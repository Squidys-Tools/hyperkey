use crate::synth::{ModifierSynthesizer, SYNTHETIC_INPUT_TAG};
use hyperkey_core::keys::{KeyTransition, KeyboardEvent, TriggerKey, VirtualKey};
use hyperkey_core::machine::{self, InputDecision, TriggerMachineState};
use hyperkey_core::settings::OutputModifier;
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE,
    PeekMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum InputEngineStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

/// State the hook callback and the control surface share. SendInput inside
/// the callback delivers the injected events to this same callback before it
/// returns, so the callback must never wait on `shared` for its own generated
/// input — `dispatch_hook_event` handles that. Lock order: the callback takes
/// `shared` and may briefly take `lifecycle` for status updates; nothing may
/// hold `lifecycle` while acquiring `shared`, or the control and hook threads
/// can deadlock.
struct Shared {
    machine: TriggerMachineState,
    trigger: TriggerKey,
    output_modifiers: Vec<OutputModifier>,
    synthesizer: ModifierSynthesizer,
}

struct Lifecycle {
    status: InputEngineStatus,
    status_error: Option<String>,
    hook_thread: Option<JoinHandle<()>>,
}

struct EngineInner {
    shared: Mutex<Shared>,
    lifecycle: Mutex<Lifecycle>,
    hook_ready: Mutex<bool>,
    hook_ready_cv: Condvar,
    stop_requested: AtomicBool,
    hook_thread_id: AtomicU32,
}

/// Per-hook-thread context installed before the pump starts so the FFI
/// callback can reach the engine without a global mutable static.
struct HookContext {
    hook: HHOOK,
    engine: Arc<EngineInner>,
}

thread_local! {
    static HOOK_CONTEXT: RefCell<Option<HookContext>> = const { RefCell::new(None) };
    static CALLBACK_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Tracks re-entrant callback invocations on the hook thread. SendInput inside
/// the callback delivers the injected events to this same callback before it
/// returns, so nested dispatch must never block on `shared`.
struct CallbackDepth;

impl CallbackDepth {
    fn enter() -> Self {
        CALLBACK_DEPTH.with(|depth| depth.set(depth.get() + 1));
        Self
    }

    fn is_nested() -> bool {
        CALLBACK_DEPTH.with(|depth| depth.get() > 1)
    }
}

impl Drop for CallbackDepth {
    fn drop(&mut self) {
        CALLBACK_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

pub struct InputEngine {
    inner: Arc<EngineInner>,
    disposed: AtomicBool,
}

impl InputEngine {
    pub fn new(trigger_key: TriggerKey, output_modifiers: Vec<OutputModifier>) -> Self {
        Self {
            inner: Arc::new(EngineInner {
                shared: Mutex::new(Shared {
                    machine: TriggerMachineState::IDLE,
                    trigger: trigger_key,
                    output_modifiers,
                    synthesizer: ModifierSynthesizer::new(),
                }),
                lifecycle: Mutex::new(Lifecycle {
                    status: InputEngineStatus::Stopped,
                    status_error: None,
                    hook_thread: None,
                }),
                hook_ready: Mutex::new(false),
                hook_ready_cv: Condvar::new(),
                stop_requested: AtomicBool::new(false),
                hook_thread_id: AtomicU32::new(0),
            }),
            disposed: AtomicBool::new(false),
        }
    }

    pub fn configure(&self, trigger_key: TriggerKey, output_modifiers: Vec<OutputModifier>) {
        assert!(
            !self.disposed.load(Ordering::SeqCst),
            "InputEngine is disposed."
        );

        let mut shared = lock(&self.inner.shared);
        shared.synthesizer.release_all(&shared.output_modifiers);
        shared.machine = TriggerMachineState::IDLE;
        shared.trigger = trigger_key;
        shared.output_modifiers = output_modifiers;
    }

    pub fn status(&self) -> InputEngineStatus {
        lock(&self.inner.lifecycle).status
    }

    pub fn status_error(&self) -> Option<String> {
        lock(&self.inner.lifecycle).status_error.clone()
    }

    pub fn start(&self) -> bool {
        {
            let mut lifecycle = lock(&self.inner.lifecycle);
            assert!(
                !self.disposed.load(Ordering::SeqCst),
                "InputEngine is disposed."
            );

            match lifecycle.status {
                InputEngineStatus::Running => return true,
                InputEngineStatus::Starting | InputEngineStatus::Stopping => return false,
                _ => {}
            }

            if let Some(handle) = lifecycle.hook_thread.as_ref() {
                if !handle.is_finished() {
                    return false;
                }

                lifecycle.hook_thread = None;
                self.inner.hook_thread_id.store(0, Ordering::SeqCst);
            }

            self.inner.stop_requested.store(false, Ordering::SeqCst);
            *lock(&self.inner.hook_ready) = false;
            self.inner.hook_thread_id.store(0, Ordering::SeqCst);
            set_status_locked(&mut lifecycle, InputEngineStatus::Starting, None);

            let inner = Arc::clone(&self.inner);
            let spawned = std::thread::Builder::new()
                .name("Hyperkey keyboard hook".to_string())
                .spawn(move || hook_thread_main(inner));
            match spawned {
                Ok(handle) => lifecycle.hook_thread = Some(handle),
                Err(error) => {
                    set_status_locked(
                        &mut lifecycle,
                        InputEngineStatus::Failed,
                        Some(format!("The keyboard hook thread could not be started. {error}")),
                    );
                    return false;
                }
            }
        }

        if !wait_for_hook_ready(&self.inner, Duration::from_secs(3)) {
            self.stop();
            return false;
        }

        self.status() == InputEngineStatus::Running
    }

    pub fn stop(&self) {
        enum StopAction {
            AlreadyStopped,
            RequestStop { hook_thread_id: u32 },
        }

        let action = {
            let mut lifecycle = lock(&self.inner.lifecycle);
            if lifecycle.status == InputEngineStatus::Stopped && lifecycle.hook_thread.is_none() {
                StopAction::AlreadyStopped
            } else {
                self.inner.stop_requested.store(true, Ordering::SeqCst);
                set_status_locked(&mut lifecycle, InputEngineStatus::Stopping, None);
                StopAction::RequestStop {
                    hook_thread_id: self.inner.hook_thread_id.load(Ordering::SeqCst),
                }
            }
        };

        match action {
            StopAction::AlreadyStopped => {
                self.emergency_disable();
                return;
            }
            StopAction::RequestStop { hook_thread_id } => {
                self.emergency_disable();

                if hook_thread_id != 0 {
                    // SAFETY: FFI call; posts WM_QUIT to the hook thread's queue.
                    let posted = unsafe {
                        PostThreadMessageW(hook_thread_id, WM_QUIT, WPARAM(0), LPARAM(0))
                    };
                    if posted.is_err() {
                        set_status(
                            &self.inner,
                            InputEngineStatus::Stopping,
                            Some(
                                "The keyboard hook shutdown message could not be posted."
                                    .to_string(),
                            ),
                        );
                    }
                }
            }
        }

        let handle = lock(&self.inner.lifecycle).hook_thread.take();
        if let Some(handle) = handle {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !handle.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }

            if handle.is_finished() {
                let _ = handle.join();
            } else {
                lock(&self.inner.lifecycle).hook_thread = Some(handle);
            }
        }

        let mut lifecycle = lock(&self.inner.lifecycle);
        if lifecycle.hook_thread.is_none() {
            self.inner.hook_thread_id.store(0, Ordering::SeqCst);
            set_status_locked(&mut lifecycle, InputEngineStatus::Stopped, None);
        } else {
            set_status_locked(
                &mut lifecycle,
                InputEngineStatus::Failed,
                Some("The keyboard hook thread did not stop cleanly.".to_string()),
            );
        }
    }

    /// Resets the machine to Idle and releases every generated modifier.
    /// Deliberately does not persist a disabled state.
    pub fn emergency_disable(&self) {
        let mut shared = lock(&self.inner.shared);
        shared.machine = TriggerMachineState::IDLE;
        shared.synthesizer.release_all(&shared.output_modifiers);
    }
}

impl Drop for InputEngine {
    fn drop(&mut self) {
        if self.disposed.swap(true, Ordering::SeqCst) {
            return;
        }

        self.stop();
    }
}

fn hook_thread_main(inner: Arc<EngineInner>) {
    let installed = install_and_pump(&inner);

    HOOK_CONTEXT.with(|cell| *cell.borrow_mut() = None);
    if let Some(hook) = installed {
        // SAFETY: FFI cleanup for the hook this thread installed.
        let _ = unsafe { UnhookWindowsHookEx(hook) };
    }

    emergency_disable(&inner);

    {
        let mut lifecycle = lock(&inner.lifecycle);
        inner.hook_thread_id.store(0, Ordering::SeqCst);
        if !inner.stop_requested.load(Ordering::SeqCst)
            && lifecycle.status != InputEngineStatus::Failed
        {
            set_status_locked(
                &mut lifecycle,
                InputEngineStatus::Failed,
                Some("The keyboard hook stopped unexpectedly.".to_string()),
            );
        }
    }

    signal_hook_ready(&inner);
}

fn install_and_pump(inner: &Arc<EngineInner>) -> Option<HHOOK> {
    // SAFETY: FFI query with no preconditions.
    inner
        .hook_thread_id
        .store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);

    // Force the thread's message queue into existence so PostThreadMessage can
    // reach it before the pump starts.
    let mut message = MSG::default();
    // SAFETY: FFI call; `message` is a valid out-parameter for the call.
    let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) };

    if inner.stop_requested.load(Ordering::SeqCst) {
        signal_hook_ready(inner);
        return None;
    }

    // SAFETY: FFI query for this process's module handle; no retained state.
    let module = unsafe { GetModuleHandleW(None) };
    let hook = unsafe {
        // SAFETY: FFI install; the callback is a plain extern fn with no
        // captured state, and the returned handle is unhooked on this thread.
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(hook_callback),
            module.ok().map(|module| HINSTANCE(module.0)),
            0,
        )
    };
    let hook = match hook {
        Ok(hook) => hook,
        Err(error) => {
            set_status(
                inner,
                InputEngineStatus::Failed,
                Some(format!(
                    "The low-level keyboard hook could not be installed. {error}"
                )),
            );
            signal_hook_ready(inner);
            return None;
        }
    };

    HOOK_CONTEXT.with(|cell| {
        *cell.borrow_mut() = Some(HookContext {
            hook,
            engine: Arc::clone(inner),
        });
    });

    set_status(inner, InputEngineStatus::Running, None);
    signal_hook_ready(inner);

    while !inner.stop_requested.load(Ordering::SeqCst) {
        let mut message = MSG::default();
        // SAFETY: FFI call; `message` is a valid out-parameter. BOOL is -1 on
        // error, 0 on WM_QUIT, positive otherwise.
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            if result.0 < 0 && !inner.stop_requested.load(Ordering::SeqCst) {
                set_status(
                    inner,
                    InputEngineStatus::Failed,
                    Some("The keyboard hook message loop failed.".to_string()),
                );
            }

            break;
        }

        // SAFETY: FFI calls on the message this thread just retrieved.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    Some(hook)
}

/// The low-level keyboard callback. Runs on the hook thread during message
/// dispatch, must do minimal work, and must never unwind across the FFI
/// boundary — a panic becomes a failed status plus a pass-through.
unsafe extern "system" fn hook_callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let context = HOOK_CONTEXT.with(|cell| match cell.try_borrow() {
        Ok(cell) => cell
            .as_ref()
            .map(|context| (context.hook, Arc::clone(&context.engine))),
        Err(_) => None,
    });

    let Some((hook, engine)) = context else {
        // SAFETY: FFI call; unhandled events must always be chained.
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    if code < 0 {
        // SAFETY: FFI call; negative codes must be forwarded untouched.
        return unsafe { CallNextHookEx(Some(hook), code, wparam, lparam) };
    }

    let result = catch_unwind(AssertUnwindSafe(|| {
        let _depth = CallbackDepth::enter();
        dispatch_hook_event(&engine, hook, code, wparam, lparam)
    }));

    match result {
        Ok(result) => result,
        Err(_) => {
            emergency_disable(&engine);
            set_status(
                &engine,
                InputEngineStatus::Failed,
                Some("The keyboard hook callback failed.".to_string()),
            );
            // SAFETY: FFI call; a panicking event falls back to pass-through.
            unsafe { CallNextHookEx(Some(hook), code, wparam, lparam) }
        }
    }
}

fn dispatch_hook_event(
    engine: &Arc<EngineInner>,
    hook: HHOOK,
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let Some(transition) = try_get_transition(wparam.0 as u32) else {
        // SAFETY: FFI call; non-key messages are forwarded untouched.
        return unsafe { CallNextHookEx(Some(hook), code, wparam, lparam) };
    };

    // SAFETY: Windows hands the callback a valid KBDLLHOOKSTRUCT for HC_ACTION.
    let native = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let input = KeyboardEvent {
        key: VirtualKey(native.vkCode as u16),
        transition,
        is_synthetic: native.dwExtraInfo as u64 == SYNTHETIC_INPUT_TAG,
    };

    if input.is_synthetic {
        // SendInput below delivers our own tagged events back to this callback
        // before it returns; the machine would pass them through without a
        // state change anyway, so they skip `shared` and can never deadlock
        // the hook thread against itself.
        // SAFETY: FFI call; generated events are always chained untouched.
        return unsafe { CallNextHookEx(Some(hook), code, wparam, lparam) };
    }

    // A nested callback that somehow reaches real input while `shared` is
    // already held by this thread falls back to pass-through rather than
    // self-deadlocking; the outer call blocks on the lock like the C# Monitor.
    let mut shared = if CallbackDepth::is_nested() {
        match engine.shared.try_lock() {
            Ok(shared) => shared,
            Err(_) => {
                // SAFETY: FFI call; a nested event that cannot update the
                // machine is forwarded rather than deadlocking the hook.
                return unsafe { CallNextHookEx(Some(hook), code, wparam, lparam) };
            }
        }
    } else {
        lock(&engine.shared)
    };
    let trigger = shared.trigger;
    let result = machine::process_with_trigger(
        shared.machine,
        &input,
        trigger,
        &shared.output_modifiers,
    );
    shared.machine = result.state;

    match result.decision {
        InputDecision::PassThrough | InputDecision::Forward => {
            // SAFETY: FFI call; forward the event to the rest of the chain.
            unsafe { CallNextHookEx(Some(hook), code, wparam, lparam) }
        }
        InputDecision::Suppress => LRESULT(1),
        InputDecision::PressAndForward(modifiers) => {
            match shared.synthesizer.try_press(&modifiers) {
                Ok(()) => {
                    // SAFETY: FFI call; modifiers are held, key goes through.
                    unsafe { CallNextHookEx(Some(hook), code, wparam, lparam) }
                }
                Err(error) => {
                    shared.machine = TriggerMachineState::IDLE;
                    set_status(engine, InputEngineStatus::Failed, Some(error));
                    LRESULT(1)
                }
            }
        }
        InputDecision::ReleaseAndSuppress(modifiers) => {
            if let Err(error) = shared.synthesizer.try_release(&modifiers) {
                set_status(engine, InputEngineStatus::Failed, Some(error));
            }

            LRESULT(1)
        }
        InputDecision::ReplayTrigger => {
            if let Err(error) = shared.synthesizer.try_replay_trigger(trigger) {
                set_status(engine, InputEngineStatus::Failed, Some(error));
            }

            // The physical key-down was already suppressed, so the physical
            // key-up stays suppressed even if Windows rejects the replay.
            LRESULT(1)
        }
    }
}

fn try_get_transition(message: u32) -> Option<KeyTransition> {
    match message {
        WM_KEYDOWN | WM_SYSKEYDOWN => Some(KeyTransition::Down),
        WM_KEYUP | WM_SYSKEYUP => Some(KeyTransition::Up),
        _ => None,
    }
}

fn emergency_disable(engine: &Arc<EngineInner>) {
    let mut shared = lock(&engine.shared);
    shared.machine = TriggerMachineState::IDLE;
    shared.synthesizer.release_all(&shared.output_modifiers);
}

fn set_status(engine: &Arc<EngineInner>, status: InputEngineStatus, error: Option<String>) {
    let mut lifecycle = lock(&engine.lifecycle);
    set_status_locked(&mut lifecycle, status, error);
}

fn set_status_locked(
    lifecycle: &mut Lifecycle,
    status: InputEngineStatus,
    error: Option<String>,
) {
    lifecycle.status = status;
    lifecycle.status_error = error;
}

fn signal_hook_ready(engine: &Arc<EngineInner>) {
    *lock(&engine.hook_ready) = true;
    engine.hook_ready_cv.notify_all();
}

fn wait_for_hook_ready(engine: &Arc<EngineInner>, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    let mut ready = lock(&engine.hook_ready);
    while !*ready {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }

        let (guard, _) = engine
            .hook_ready_cv
            .wait_timeout(ready, remaining)
            .unwrap_or_else(|error| error.into_inner());
        ready = guard;
    }

    true
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A poisoned lock still holds usable state; the engine recovers rather
    // than dropping pressed modifiers or losing the machine phase.
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}
