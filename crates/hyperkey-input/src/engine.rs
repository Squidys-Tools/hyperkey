use crate::synth::{ModifierSynthesizer, SYNTHETIC_INPUT_TAG, is_heartbeat_probe};
use hyperkey_core::keys::{KeyTransition, KeyboardEvent, TriggerKey, VirtualKey};
use hyperkey_core::machine::{self, InputDecision, TriggerMachineState};
use hyperkey_core::settings::OutputModifier;
use hyperkey_core::watchdog::{HookWatchdog, HookWatchdogAction};
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount64};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE,
    PeekMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_USER,
};

/// Posted to the hook thread's queue when the hook needs reinstalling. Only
/// the thread that owns the message loop may call `UnhookWindowsHookEx` and
/// `SetWindowsHookEx`, so the request travels as a thread message. `wParam`
/// is 1 when the caller asked for a forced restart (`restart_hook`) and 0 for
/// a watchdog-detected dead hook.
const WM_REINSTALL_HOOK: u32 = WM_USER + 0x2A;

/// How often the watchdog decides between probing and reinstalling.
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(5);
/// A heartbeat older than this is treated as a dead (silently removed) hook.
const WATCHDOG_DEAD_AFTER_MS: u64 = 15_000;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum InputEngineStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

/// A lifecycle notification delivered to every `subscribe` receiver.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EngineEvent {
    /// The engine status changed; `error` carries the reason on transitions
    /// to `Failed` and other error states.
    StatusChanged {
        status: InputEngineStatus,
        error: Option<String>,
    },
    /// The watchdog reinstalled a silently removed keyboard hook; `message`
    /// is a user-facing description of the automatic recovery.
    HookRecovered { message: String },
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
    last_recovery_message: Option<String>,
    hook_thread: Option<JoinHandle<()>>,
    watchdog_control: Option<WatchdogControl>,
    event_senders: Vec<mpsc::Sender<EngineEvent>>,
}

/// Keeps the watchdog thread alive and gives it a shutdown signal. Dropping
/// `stop` disconnects the channel, which ends the thread's `recv_timeout`
/// wait immediately.
struct WatchdogControl {
    stop: mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

struct EngineInner {
    shared: Mutex<Shared>,
    lifecycle: Mutex<Lifecycle>,
    /// The active watchdog, taken briefly to clone the `Arc`; the heartbeat
    /// itself is an atomic write so the hook callback stays cheap.
    watchdog: Mutex<Option<Arc<HookWatchdog>>>,
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
                    last_recovery_message: None,
                    hook_thread: None,
                    watchdog_control: None,
                    event_senders: Vec::new(),
                }),
                watchdog: Mutex::new(None),
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

    /// The message recorded by the most recent automatic hook reinstall, if
    /// the watchdog has ever had to heal a silently removed hook.
    pub fn last_recovery_message(&self) -> Option<String> {
        lock(&self.inner.lifecycle).last_recovery_message.clone()
    }

    /// Subscribes to engine lifecycle events: every status transition and
    /// every automatic hook recovery, in engine order. Dropping the receiver
    /// unsubscribes it.
    pub fn subscribe(&self) -> mpsc::Receiver<EngineEvent> {
        let (sender, receiver) = mpsc::channel();
        lock(&self.inner.lifecycle).event_senders.push(sender);
        receiver
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
            // Publish while still holding the lock and before the hook thread
            // exists, so subscribers can never see Running before Starting.
            publish_status_locked(&mut lifecycle, InputEngineStatus::Starting, None);

            let inner = Arc::clone(&self.inner);
            let spawned = std::thread::Builder::new()
                .name("Hyperkey keyboard hook".to_string())
                .spawn(move || hook_thread_main(inner));
            match spawned {
                Ok(handle) => lifecycle.hook_thread = Some(handle),
                Err(error) => {
                    let message =
                        format!("The keyboard hook thread could not be started. {error}");
                    set_status_locked(
                        &mut lifecycle,
                        InputEngineStatus::Failed,
                        Some(message.clone()),
                    );
                    publish_status_locked(&mut lifecycle, InputEngineStatus::Failed, Some(message));
                    return false;
                }
            }
        }

        if !wait_for_hook_ready(&self.inner, Duration::from_secs(3)) {
            self.stop();
            return false;
        }

        if self.status() != InputEngineStatus::Running {
            return false;
        }

        self.start_watchdog();
        self.status() == InputEngineStatus::Running
    }

    pub fn stop(&self) {
        self.stop_watchdog();

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

        let hook_thread_id = match action {
            StopAction::AlreadyStopped => {
                self.emergency_disable();
                return;
            }
            StopAction::RequestStop { hook_thread_id } => hook_thread_id,
        };

        publish_status(&self.inner, InputEngineStatus::Stopping, None);

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

        let final_status;
        let final_error;
        {
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

            final_status = lifecycle.status;
            final_error = lifecycle.status_error.clone();
        }

        publish_status(&self.inner, final_status, final_error);
    }

    /// Resets the machine to Idle and releases every generated modifier.
    /// Deliberately does not persist a disabled state.
    pub fn emergency_disable(&self) {
        reset_shared_state(&self.inner);
    }

    /// Phase-3 recovery: releases any generated modifiers and resets the
    /// state machine, exactly like `emergency_disable`. The app calls this
    /// after sleep/resume, workstation lock/unlock, or a session change so a
    /// modifier layer generated before the transition cannot stick.
    pub fn reconcile(&self) {
        reset_shared_state(&self.inner);
    }

    /// Asks the hook thread to drop and reinstall its low-level keyboard
    /// hook. Returns false when the engine is not `Running` (start/stop own
    /// hook lifetime then) or the request could not be posted. The reinstall
    /// is forced: it runs even while heartbeats still arrive, unlike a
    /// watchdog-queued request, which is deduped away when the hook proves
    /// alive.
    pub fn restart_hook(&self) -> bool {
        assert!(
            !self.disposed.load(Ordering::SeqCst),
            "InputEngine is disposed."
        );

        if self.status() != InputEngineStatus::Running {
            return false;
        }

        let hook_thread_id = self.inner.hook_thread_id.load(Ordering::SeqCst);
        if hook_thread_id == 0 {
            return false;
        }

        // SAFETY: FFI call; posts the forced reinstall request to the hook
        // thread's queue.
        unsafe {
            PostThreadMessageW(hook_thread_id, WM_REINSTALL_HOOK, WPARAM(1), LPARAM(0))
        }
        .is_ok()
    }

    /// Spawns the watchdog thread and installs the shared watchdog state.
    /// The watchdog only exists while the engine is `Running`.
    fn start_watchdog(&self) {
        self.stop_watchdog();

        if self.disposed.load(Ordering::SeqCst) || self.inner.stop_requested.load(Ordering::SeqCst) {
            return;
        }

        let watchdog = Arc::new(HookWatchdog::new(WATCHDOG_DEAD_AFTER_MS, tick_count()));
        let (stop, stop_rx) = mpsc::channel::<()>();
        let inner = Arc::clone(&self.inner);
        let spawned = std::thread::Builder::new()
            .name("Hyperkey hook watchdog".to_string())
            .spawn(move || watchdog_thread_main(inner, stop_rx));

        match spawned {
            Ok(thread) => {
                *lock(&self.inner.watchdog) = Some(watchdog);
                lock(&self.inner.lifecycle).watchdog_control =
                    Some(WatchdogControl { stop, thread });
            }
            Err(error) => {
                // Without a watchdog a running hook has no self-heal, so stop
                // the engine instead of leaving it running but flagged Failed
                // with no way forward but a manual stop().
                self.stop();
                set_status(
                    &self.inner,
                    InputEngineStatus::Failed,
                    Some(format!("The hook watchdog could not be started. {error}")),
                );
            }
        }
    }

    /// Clears the watchdog and joins its thread. Dropping the sender wakes
    /// the thread through a disconnected `recv_timeout`.
    fn stop_watchdog(&self) {
        *lock(&self.inner.watchdog) = None;
        let control = lock(&self.inner.lifecycle).watchdog_control.take();
        if let Some(control) = control {
            drop(control.stop);
            let _ = control.thread.join();
        }
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
    // The pump runs under catch_unwind for the same reason C# wraps it in
    // try/catch/finally: a failure must still surface as `Failed` and still
    // unhook. On a panic the context keeps the last installed handle, so the
    // cleanup below can release it instead of leaking the hook.
    let pumped = catch_unwind(AssertUnwindSafe(|| install_and_pump(&inner)));
    if pumped.is_err() && !inner.stop_requested.load(Ordering::SeqCst) {
        set_status(
            &inner,
            InputEngineStatus::Failed,
            Some("The keyboard hook thread failed.".to_string()),
        );
    }

    let context_hook = HOOK_CONTEXT
        .with(|cell| cell.borrow_mut().take())
        .map(|context| context.hook);
    // A pump that ended normally reports the hook it left installed; a pump
    // that panicked left it in the context. Either way it is unhooked here —
    // re-unhooking an already removed handle is a harmless error.
    let installed = pumped.ok().flatten().or(context_hook);
    if let Some(hook) = installed {
        // SAFETY: FFI cleanup for the hook this thread installed.
        let _ = unsafe { UnhookWindowsHookEx(hook) };
    }

    emergency_disable(&inner);

    let mut stopped_unexpectedly = false;
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
            stopped_unexpectedly = true;
        }
    }

    if stopped_unexpectedly {
        publish_status(
            &inner,
            InputEngineStatus::Failed,
            Some("The keyboard hook stopped unexpectedly.".to_string()),
        );
    }

    signal_hook_ready(&inner);
}

/// Installs the hook, reports `Running`, then pumps the thread's message
/// queue — `WH_KEYBOARD_LL` callbacks only fire while this loop dispatches
/// messages. `WM_REINSTALL_HOOK` messages re-install the hook in place.
/// Returns the hook left installed when the loop exits (or `None`), which the
/// caller unhooks.
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

    let mut current_hook = match try_install_hook() {
        Ok(hook) => Some(hook),
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
            hook: current_hook.expect("installed hook is stored in the context"),
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

        if message.message == WM_REINSTALL_HOOK {
            if !reinstall_hook(inner, &mut current_hook, message.wParam.0 != 0) {
                break;
            }

            continue;
        }

        // SAFETY: FFI calls on the message this thread just retrieved.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    current_hook
}

// SAFETY: FFI install; the callback is a plain extern fn with no captured
// state, and the returned handle is unhooked on the installing thread.
fn try_install_hook() -> windows::core::Result<HHOOK> {
    // SAFETY: FFI query for this process's module handle; no retained state.
    let module = unsafe { GetModuleHandleW(None) };
    unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(hook_callback),
            module.ok().map(|module| HINSTANCE(module.0)),
            0,
        )
    }
}

/// Runs on the hook thread inside the pump. Reinstall requests can pile up
/// while the hook thread is stalled, so a queued request first re-checks the
/// watchdog: a heartbeat that arrived after the request was posted proves the
/// hook is alive again and the leftover has nothing to repair. `forced`
/// requests (from `restart_hook`) always reinstall. Returns false when the
/// pump must exit — a failed reinstall is unrecoverable and already surfaced
/// as a `Failed` status.
fn reinstall_hook(
    inner: &Arc<EngineInner>,
    current_hook: &mut Option<HHOOK>,
    forced: bool,
) -> bool {
    let watchdog = lock(&inner.watchdog).clone();
    if inner.stop_requested.load(Ordering::SeqCst) {
        return true;
    }
    let Some(watchdog) = watchdog else {
        return true;
    };

    if !forced && watchdog.tick(tick_count()) == HookWatchdogAction::SendProbe {
        return true;
    }

    if let Some(hook) = current_hook.take() {
        // SAFETY: FFI cleanup for the hook this thread installed.
        let _ = unsafe { UnhookWindowsHookEx(hook) };
    }

    emergency_disable(inner);

    match try_install_hook() {
        Err(error) => {
            set_status(
                inner,
                InputEngineStatus::Failed,
                Some(format!(
                    "The low-level keyboard hook could not be reinstalled. {error}"
                )),
            );
            false
        }
        Ok(hook) => {
            *current_hook = Some(hook);
            HOOK_CONTEXT.with(|cell| {
                if let Some(context) = cell.borrow_mut().as_mut() {
                    context.hook = hook;
                }
            });
            watchdog.reset(tick_count());
            record_recovery(
                inner,
                format!(
                    "The keyboard hook was reinstalled automatically at {} after it stopped responding.",
                    local_time_stamp()
                ),
            );
            true
        }
    }
}

/// The watchdog loop: every interval, decide between probing the hook and
/// requesting its reinstall. Exits when the control channel disconnects.
fn watchdog_thread_main(inner: Arc<EngineInner>, stop: mpsc::Receiver<()>) {
    loop {
        match stop.recv_timeout(WATCHDOG_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => watchdog_tick(&inner),
        }
    }
}

fn watchdog_tick(inner: &Arc<EngineInner>) {
    if inner.stop_requested.load(Ordering::SeqCst)
        || lock(&inner.lifecycle).status != InputEngineStatus::Running
    {
        return;
    }

    let Some(watchdog) = lock(&inner.watchdog).clone() else {
        return;
    };

    match watchdog.tick(tick_count()) {
        HookWatchdogAction::SendProbe => {
            lock(&inner.shared).synthesizer.try_send_heartbeat_probe();
        }
        HookWatchdogAction::ReinstallHook => {
            // The hook can only be uninstalled and reinstalled on the thread
            // that owns the message loop, so hand the work to it. A post
            // failure resets the watchdog so this does not spam a dead queue
            // every tick.
            let hook_thread_id = inner.hook_thread_id.load(Ordering::SeqCst);
            let posted = hook_thread_id != 0
                && unsafe {
                    PostThreadMessageW(hook_thread_id, WM_REINSTALL_HOOK, WPARAM(0), LPARAM(0))
                }
                .is_ok();
            if !posted {
                watchdog.reset(tick_count());
            }
        }
    }
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

    // Every event the callback sees proves the hook is still attached; the
    // watchdog feeds on these beats, and a live probe counts too.
    if let Some(watchdog) = lock(&engine.watchdog).clone() {
        watchdog.heartbeat(tick_count());
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
    // SAFETY: Windows hands the callback a valid KBDLLHOOKSTRUCT for HC_ACTION.
    let native = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };

    if is_heartbeat_probe(native.vkCode, native.dwExtraInfo) {
        // A watchdog probe only proves the hook is still alive; suppress it
        // so the beat never reaches the rest of the system — and keep it out
        // of the trigger state machine entirely.
        return LRESULT(1);
    }

    let Some(transition) = try_get_transition(wparam.0 as u32) else {
        // SAFETY: FFI call; non-key messages are forwarded untouched.
        return unsafe { CallNextHookEx(Some(hook), code, wparam, lparam) };
    };

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

fn reset_shared_state(inner: &Arc<EngineInner>) {
    let mut shared = lock(&inner.shared);
    shared.machine = TriggerMachineState::IDLE;
    shared.synthesizer.release_all(&shared.output_modifiers);
}

fn emergency_disable(inner: &Arc<EngineInner>) {
    reset_shared_state(inner);
}

fn record_recovery(inner: &Arc<EngineInner>, message: String) {
    {
        let mut lifecycle = lock(&inner.lifecycle);
        lifecycle.last_recovery_message = Some(message.clone());
    }

    publish(inner, EngineEvent::HookRecovered { message });
}

fn set_status(engine: &Arc<EngineInner>, status: InputEngineStatus, error: Option<String>) {
    {
        let mut lifecycle = lock(&engine.lifecycle);
        set_status_locked(&mut lifecycle, status, error.clone());
    }

    publish_status(engine, status, error);
}

fn set_status_locked(
    lifecycle: &mut Lifecycle,
    status: InputEngineStatus,
    error: Option<String>,
) {
    lifecycle.status = status;
    lifecycle.status_error = error;
}

/// Sends an event to every subscriber and prunes receivers that went away.
/// `mpsc::Sender::send` never blocks, so this is safe under `lifecycle`.
fn publish(engine: &Arc<EngineInner>, event: EngineEvent) {
    let mut lifecycle = lock(&engine.lifecycle);
    publish_locked(&mut lifecycle, event);
}

/// Same as `publish`, for a caller already holding `lifecycle` — used where
/// the event must be sequenced under the lock that changed the status.
fn publish_locked(lifecycle: &mut Lifecycle, event: EngineEvent) {
    lifecycle
        .event_senders
        .retain(|sender| sender.send(event.clone()).is_ok());
}

fn publish_status_locked(
    lifecycle: &mut Lifecycle,
    status: InputEngineStatus,
    error: Option<String>,
) {
    publish_locked(lifecycle, EngineEvent::StatusChanged { status, error });
}

fn publish_status(engine: &Arc<EngineInner>, status: InputEngineStatus, error: Option<String>) {
    publish(engine, EngineEvent::StatusChanged { status, error });
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

/// Milliseconds since boot, the same monotonic (wrapping) tick source the C#
/// engine uses via `Environment.TickCount64`.
fn tick_count() -> u64 {
    // SAFETY: FFI query with no preconditions.
    unsafe { GetTickCount64() }
}

/// "HH:MM:SS" in local time, for the recovery message's `DateTime.Now:T`.
fn local_time_stamp() -> String {
    // SAFETY: FFI query with no preconditions.
    let time = unsafe { GetLocalTime() };
    format!("{:02}:{:02}:{:02}", time.wHour, time.wMinute, time.wSecond)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A poisoned lock still holds usable state; the engine recovers rather
    // than dropping pressed modifiers or losing the machine phase.
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> InputEngine {
        InputEngine::new(TriggerKey::CAPS_LOCK, vec![OutputModifier::Control])
    }

    #[test]
    fn reconcile_resets_the_machine_to_idle() {
        let engine = engine();
        lock(&engine.inner.shared).machine = TriggerMachineState {
            phase: machine::TriggerPhase::HyperActive,
        };

        engine.reconcile();

        assert_eq!(
            TriggerMachineState::IDLE,
            lock(&engine.inner.shared).machine
        );
    }

    #[test]
    fn emergency_disable_resets_the_machine_to_idle() {
        let engine = engine();
        lock(&engine.inner.shared).machine = TriggerMachineState {
            phase: machine::TriggerPhase::TriggerHeld,
        };

        engine.emergency_disable();

        assert_eq!(
            TriggerMachineState::IDLE,
            lock(&engine.inner.shared).machine
        );
    }

    #[test]
    fn restart_hook_returns_false_when_not_running() {
        let engine = engine();

        assert!(!engine.restart_hook());
        assert_eq!(InputEngineStatus::Stopped, engine.status());
    }

    #[test]
    fn watchdog_tick_resets_a_dead_hook_when_the_request_cannot_post() {
        // With no hook thread, hook_thread_id is 0 so the reinstall request
        // can never be posted; the watchdog must reset instead of spamming
        // the dead queue on every tick.
        let engine = engine();
        {
            let mut lifecycle = lock(&engine.inner.lifecycle);
            lifecycle.status = InputEngineStatus::Running;
        }
        let watchdog = Arc::new(HookWatchdog::new(50, 0));
        *lock(&engine.inner.watchdog) = Some(Arc::clone(&watchdog));

        assert_eq!(
            HookWatchdogAction::ReinstallHook,
            watchdog.tick(tick_count())
        );
        watchdog_tick(&engine.inner);
        // The failed post reset the watchdog, so the next tick probes again.
        assert_eq!(HookWatchdogAction::SendProbe, watchdog.tick(tick_count()));

        engine.stop();
    }

    #[test]
    fn subscribe_receives_status_events() {
        let engine = engine();
        let receiver = engine.subscribe();

        set_status(&engine.inner, InputEngineStatus::Failed, Some("boom".to_string()));

        assert_eq!(
            EngineEvent::StatusChanged {
                status: InputEngineStatus::Failed,
                error: Some("boom".to_string()),
            },
            receiver.recv_timeout(Duration::from_secs(1)).unwrap()
        );
    }
}
