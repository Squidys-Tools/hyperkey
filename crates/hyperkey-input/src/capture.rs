//! One-shot key capture for trigger rebinding.
//!
//! The settings window cannot rely on its UI toolkit to report lock keys such
//! as Caps Lock, and guessing virtual keys from characters is layout-fragile.
//! `KeyCapture` instead installs its own `WH_KEYBOARD_LL` hook, so the
//! reported virtual key is exactly what the engine's hook will later see.
//! While the window is foreground the captured keys are suppressed on both
//! edges, so a captured Caps Lock does not toggle the lock.

use crate::synth::{HEARTBEAT_INPUT_TAG, SYNTHETIC_INPUT_TAG};
use hyperkey_core::keys::KeyTransition;
use std::cell::RefCell;
use std::os::windows::io::AsRawHandle;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;
use windows::Win32::Foundation::{HANDLE, HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId, GetThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
    HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG, PM_NOREMOVE, PeekMessageW, PostQuitMessage,
    PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL,
    WM_APP, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

/// How long `KeyCapture::start` waits for the hook thread to install its hook.
const START_TIMEOUT: Duration = Duration::from_secs(2);

/// Posted by `Drop`: stop capturing, then exit once every swallowed key is up.
const WM_CAPTURE_RELEASE: u32 = WM_APP + 1;

const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;
/// Unassigned virtual key that AutoHotkey and input-automation tools send as
/// a "menu mask" around modifier presses. No physical key produces it, so the
/// capture lets it through without reporting it.
const VK_MASK_KEY: u16 = 0xE8;

/// A key press captured by [`KeyCapture`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CapturedKey {
    /// The virtual key the engine's hook would see for this press.
    pub virtual_key: u16,
    /// The hook's extended-key flag (arrows, right Ctrl/Alt, numpad Enter, ...).
    pub extended: bool,
    /// Shift, Ctrl, Alt, or Win was already down when this key went down.
    pub modifiers_held: bool,
}

/// A live capture. Dropping it stops capturing at once; the hook stays only
/// until the keys it swallowed are released, so their key-ups never reach the
/// window (a captured Space or Enter would otherwise click the focused button).
pub struct KeyCapture {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl KeyCapture {
    /// Spawns a "Hyperkey key capture" thread with its own `WH_KEYBOARD_LL`
    /// hook and message loop, then returns once the hook is installed (or an
    /// `Err` with a user-facing message if the install fails or takes longer
    /// than about two seconds).
    ///
    /// `on_key` runs on the hook thread for every fresh key-down while a
    /// window of this process is foreground; it must return quickly (forward
    /// to a channel), because Windows silently removes slow low-level hooks.
    pub fn start(on_key: impl Fn(CapturedKey) + Send + 'static) -> Result<KeyCapture, String> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let abandoned = Arc::new(AtomicBool::new(false));
        let thread_abandoned = Arc::clone(&abandoned);

        let thread = std::thread::Builder::new()
            .name("Hyperkey key capture".to_string())
            .spawn(move || capture_thread_main(Box::new(on_key), ready_tx, thread_abandoned))
            .map_err(|error| format!("The key capture thread could not be started. {error}"))?;

        match ready_rx.recv_timeout(START_TIMEOUT) {
            Ok(Ok(thread_id)) => Ok(KeyCapture {
                thread_id,
                thread: Some(thread),
            }),
            Ok(Err(message)) => {
                let _ = thread.join();
                Err(message)
            }
            Err(_) => {
                // The thread may still be coming up. Tell it to give up, and
                // wake it in case it is already pumping.
                abandoned.store(true, Ordering::SeqCst);
                // SAFETY: FFI query on a thread handle owned by `thread`.
                let thread_id = unsafe { GetThreadId(HANDLE(thread.as_raw_handle())) };
                if thread_id != 0 {
                    post_quit(thread_id);
                }
                Err("The key capture hook did not start in time.".to_string())
            }
        }
    }
}

impl Drop for KeyCapture {
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };

        // The thread exits on its own once the swallowed keys are up, so it is
        // not joined here; one that already exited joins immediately.
        // SAFETY: FFI call; posts a plain message to the capture thread's queue.
        let posted =
            unsafe { PostThreadMessageW(self.thread_id, WM_CAPTURE_RELEASE, WPARAM(0), LPARAM(0)) }
                .is_ok();
        if !posted || thread.is_finished() {
            let _ = thread.join();
        }
    }
}

fn post_quit(thread_id: u32) -> bool {
    // SAFETY: FFI call; posts WM_QUIT to the capture thread's queue.
    unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }.is_ok()
}

/// What the hook does with the event.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum CaptureAction {
    Suppress,
    Pass,
}

/// Reported to `on_key`: a fresh key-down and whether a modifier was held.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct CaptureReport {
    modifiers_held: bool,
}

/// Keys this capture has seen go down and not yet up.
struct HeldKeys([bool; 256]);

impl HeldKeys {
    const fn new() -> Self {
        Self([false; 256])
    }

    fn contains(&self, virtual_key: u16) -> bool {
        self.0.get(virtual_key as usize).copied().unwrap_or(false)
    }

    fn set(&mut self, virtual_key: u16, down: bool) {
        if let Some(slot) = self.0.get_mut(virtual_key as usize) {
            *slot = down;
        }
    }

    fn is_empty(&self) -> bool {
        !self.0.contains(&true)
    }

    fn any_modifier(&self) -> bool {
        (0u16..=255).any(|key| self.0[key as usize] && is_modifier_vk(key))
    }
}

fn is_modifier_vk(virtual_key: u16) -> bool {
    matches!(
        virtual_key,
        VK_SHIFT | VK_CONTROL | VK_MENU | VK_LWIN | VK_RWIN | 0xA0..=0xA5
    )
}

/// The per-event decision, free of Win32 calls so it can be unit tested.
///
/// - `already_down_async`: `GetAsyncKeyState` says the key was physically down
///   before this event (for LL hooks the async state is not yet updated for
///   the current event).
/// - `modifiers_down_async`: `GetAsyncKeyState` says a modifier is down.
///
/// A key already down before capture started (for example the Enter that
/// opened the capture) keeps flowing and never reports itself; a key this
/// capture owns has its repeats and key-up suppressed.
fn decide(
    transition: KeyTransition,
    virtual_key: u16,
    generated: bool,
    foreground_is_ours: bool,
    already_down_async: bool,
    modifiers_down_async: bool,
    held: &mut HeldKeys,
) -> (CaptureAction, Option<CaptureReport>) {
    if generated || virtual_key == VK_MASK_KEY {
        return (CaptureAction::Pass, None);
    }

    if !foreground_is_ours {
        // Focus moved away mid-press: forget the key so a later press is not
        // mistaken for a repeat, but leave the event itself alone.
        if transition == KeyTransition::Up {
            held.set(virtual_key, false);
        }
        return (CaptureAction::Pass, None);
    }

    match transition {
        KeyTransition::Down => {
            if held.contains(virtual_key) {
                return (CaptureAction::Suppress, None);
            }
            if already_down_async {
                return (CaptureAction::Pass, None);
            }

            let modifiers_held = held.any_modifier() || modifiers_down_async;
            held.set(virtual_key, true);
            (
                CaptureAction::Suppress,
                Some(CaptureReport { modifiers_held }),
            )
        }
        KeyTransition::Up => {
            if held.contains(virtual_key) {
                held.set(virtual_key, false);
                (CaptureAction::Suppress, None)
            } else {
                (CaptureAction::Pass, None)
            }
        }
    }
}

struct CaptureContext {
    hook: HHOOK,
    on_key: Rc<dyn Fn(CapturedKey)>,
    held: HeldKeys,
    /// Capture ended; only key-ups of `held` keys are still swallowed.
    releasing: bool,
}

thread_local! {
    static CAPTURE_CONTEXT: RefCell<Option<CaptureContext>> = const { RefCell::new(None) };
}

fn capture_thread_main(
    on_key: Box<dyn Fn(CapturedKey) + Send>,
    ready: mpsc::Sender<Result<u32, String>>,
    abandoned: Arc<AtomicBool>,
) {
    // Force the thread's message queue into existence so WM_QUIT can reach it
    // before the pump starts.
    let mut message = MSG::default();
    // SAFETY: FFI call; `message` is a valid out-parameter for the call.
    let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) };

    let hook = match try_install_hook() {
        Ok(hook) => hook,
        Err(error) => {
            let _ = ready.send(Err(format!(
                "The low-level keyboard hook could not be installed for key capture. {error}"
            )));
            return;
        }
    };

    let on_key: Rc<dyn Fn(CapturedKey)> = Rc::from(on_key as Box<dyn Fn(CapturedKey)>);
    CAPTURE_CONTEXT.with(|cell| {
        *cell.borrow_mut() = Some(CaptureContext {
            hook,
            on_key,
            held: HeldKeys::new(),
            releasing: false,
        });
    });

    if abandoned.load(Ordering::SeqCst) {
        finish(hook);
        return;
    }

    // SAFETY: FFI query with no preconditions.
    let thread_id = unsafe { GetCurrentThreadId() };
    if ready.send(Ok(thread_id)).is_err() {
        finish(hook);
        return;
    }

    loop {
        let mut message = MSG::default();
        // SAFETY: FFI call; `message` is a valid out-parameter. BOOL is -1 on
        // error, 0 on WM_QUIT, positive otherwise.
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }
        if message.message == WM_CAPTURE_RELEASE {
            let done = CAPTURE_CONTEXT.with(|cell| {
                cell.borrow_mut().as_mut().is_none_or(|context| {
                    context.releasing = true;
                    context.held.is_empty()
                })
            });
            if done {
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

    finish(hook);
}

/// Drops the thread's context and unhooks. Runs on the installing thread.
fn finish(hook: HHOOK) {
    CAPTURE_CONTEXT.with(|cell| cell.borrow_mut().take());
    // SAFETY: FFI cleanup for the hook this thread installed.
    let _ = unsafe { UnhookWindowsHookEx(hook) };
}

fn try_install_hook() -> windows::core::Result<HHOOK> {
    // SAFETY: FFI query for this process's module handle; no retained state.
    let module = unsafe { GetModuleHandleW(None) };
    // SAFETY: FFI install; the callback is a plain extern fn with no captured
    // state, and the returned handle is unhooked on the installing thread.
    unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(hook_callback),
            module.ok().map(|module| HINSTANCE(module.0)),
            0,
        )
    }
}

/// Runs on the capture thread inside the pump; must stay tiny and must never
/// unwind across the FFI boundary.
unsafe extern "system" fn hook_callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let hook = CAPTURE_CONTEXT.with(|cell| match cell.try_borrow() {
        Ok(cell) => cell.as_ref().map(|context| context.hook),
        Err(_) => None,
    });

    let pass = |hook: Option<HHOOK>| {
        // SAFETY: FFI call; unhandled events must always be chained.
        unsafe { CallNextHookEx(hook, code, wparam, lparam) }
    };

    if code < 0 {
        return pass(hook);
    }

    let transition = match wparam.0 as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => KeyTransition::Down,
        WM_KEYUP | WM_SYSKEYUP => KeyTransition::Up,
        _ => return pass(hook),
    };

    // SAFETY: Windows hands the callback a valid KBDLLHOOKSTRUCT for HC_ACTION.
    let native = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let virtual_key = native.vkCode as u16;
    // Only Hyperkey's own tagged events are skipped, exactly like the engine
    // hook: input injected by other tools (on-screen keyboards, remote
    // sessions) is what the engine will see, so it can be captured too.
    let generated = matches!(
        native.dwExtraInfo as u64,
        SYNTHETIC_INPUT_TAG | HEARTBEAT_INPUT_TAG
    );
    let extended = native.flags.0 & LLKHF_EXTENDED.0 != 0;

    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let foreground_is_ours = !generated && foreground_is_this_process();
        let (already_down_async, modifiers_down_async) =
            if foreground_is_ours && transition == KeyTransition::Down {
                (async_down(virtual_key), any_modifier_down_async())
            } else {
                (false, false)
            };

        let (action, report, on_key) = CAPTURE_CONTEXT.with(|cell| {
            let Ok(mut cell) = cell.try_borrow_mut() else {
                return (CaptureAction::Pass, None, None);
            };
            let Some(context) = cell.as_mut() else {
                return (CaptureAction::Pass, None, None);
            };
            let fresh_press =
                transition == KeyTransition::Down && !context.held.contains(virtual_key);
            if context.releasing && fresh_press {
                return (CaptureAction::Pass, None, None);
            }
            let (action, report) = decide(
                transition,
                virtual_key,
                generated,
                foreground_is_ours,
                already_down_async,
                modifiers_down_async,
                &mut context.held,
            );
            if context.releasing && context.held.is_empty() {
                // SAFETY: FFI call on this (the capture) thread; ends its pump.
                unsafe { PostQuitMessage(0) };
            }
            (action, report, Some(Rc::clone(&context.on_key)))
        });

        // The context borrow is released before the callback runs.
        if let (Some(report), Some(on_key)) = (report, on_key) {
            on_key(CapturedKey {
                virtual_key,
                extended,
                modifiers_held: report.modifiers_held,
            });
        }

        action
    }));

    match outcome {
        Ok(CaptureAction::Suppress) => LRESULT(1),
        Ok(CaptureAction::Pass) | Err(_) => pass(hook),
    }
}

fn foreground_is_this_process() -> bool {
    // SAFETY: FFI queries with no preconditions; `process_id` is a valid
    // out-parameter for the call.
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() {
            return false;
        }

        let mut process_id = 0u32;
        if GetWindowThreadProcessId(window, Some(&mut process_id)) == 0 {
            return false;
        }

        process_id == GetCurrentProcessId()
    }
}

fn async_down(virtual_key: u16) -> bool {
    // SAFETY: FFI query with no preconditions.
    unsafe { GetAsyncKeyState(i32::from(virtual_key)) < 0 }
}

fn any_modifier_down_async() -> bool {
    [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
        .into_iter()
        .any(async_down)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: u16 = 0x41;
    const SHIFT: u16 = VK_SHIFT;

    fn down(
        key: u16,
        held: &mut HeldKeys,
        already_down_async: bool,
    ) -> (CaptureAction, Option<CaptureReport>) {
        decide(
            KeyTransition::Down,
            key,
            false,
            true,
            already_down_async,
            false,
            held,
        )
    }

    fn up(key: u16, held: &mut HeldKeys) -> (CaptureAction, Option<CaptureReport>) {
        decide(KeyTransition::Up, key, false, true, false, false, held)
    }

    #[test]
    fn first_press_reports_and_suppresses() {
        let mut held = HeldKeys::new();

        let (action, report) = down(A, &mut held, false);

        assert_eq!(CaptureAction::Suppress, action);
        assert_eq!(
            Some(CaptureReport {
                modifiers_held: false
            }),
            report
        );
        assert!(held.contains(A));
    }

    #[test]
    fn repeats_of_a_captured_key_are_suppressed_without_reporting() {
        let mut held = HeldKeys::new();
        down(A, &mut held, false);

        // Auto-repeat: the async state now says the key is down, and our own
        // held set already has it.
        assert_eq!((CaptureAction::Suppress, None), down(A, &mut held, true));
        assert_eq!((CaptureAction::Suppress, None), down(A, &mut held, false));
    }

    #[test]
    fn key_up_of_a_captured_key_is_suppressed_and_forgotten() {
        let mut held = HeldKeys::new();
        down(A, &mut held, false);

        assert_eq!((CaptureAction::Suppress, None), up(A, &mut held));
        assert!(!held.contains(A));
        // A second press reports again.
        assert!(down(A, &mut held, false).1.is_some());
    }

    #[test]
    fn repeat_of_a_key_held_before_capture_passes_and_never_reports() {
        let mut held = HeldKeys::new();

        assert_eq!((CaptureAction::Pass, None), down(0x0D, &mut held, true));
        assert!(!held.contains(0x0D));
        // Its key-up is not ours either.
        assert_eq!((CaptureAction::Pass, None), up(0x0D, &mut held));
    }

    #[test]
    fn key_up_of_an_unknown_key_passes() {
        let mut held = HeldKeys::new();

        assert_eq!((CaptureAction::Pass, None), up(A, &mut held));
    }

    #[test]
    fn foreign_foreground_passes_untouched() {
        let mut held = HeldKeys::new();

        let result = decide(
            KeyTransition::Down,
            A,
            false,
            false,
            false,
            false,
            &mut held,
        );

        assert_eq!((CaptureAction::Pass, None), result);
        assert!(!held.contains(A));
    }

    #[test]
    fn foreign_foreground_key_up_forgets_but_passes() {
        let mut held = HeldKeys::new();
        down(A, &mut held, false);

        let result = decide(KeyTransition::Up, A, false, false, false, false, &mut held);

        assert_eq!((CaptureAction::Pass, None), result);
        assert!(!held.contains(A));
    }

    #[test]
    fn hyperkey_generated_events_pass_without_reporting() {
        let mut held = HeldKeys::new();

        let down_result = decide(KeyTransition::Down, A, true, true, false, false, &mut held);
        let up_result = decide(KeyTransition::Up, A, true, true, false, false, &mut held);

        assert_eq!((CaptureAction::Pass, None), down_result);
        assert_eq!((CaptureAction::Pass, None), up_result);
        assert!(!held.contains(A));
    }

    #[test]
    fn the_mask_key_passes_without_reporting() {
        let mut held = HeldKeys::new();

        let result = decide(
            KeyTransition::Down,
            VK_MASK_KEY,
            false,
            true,
            false,
            false,
            &mut held,
        );

        assert_eq!((CaptureAction::Pass, None), result);
        assert!(!held.contains(VK_MASK_KEY));
    }

    #[test]
    fn generated_key_up_does_not_release_a_captured_key() {
        let mut held = HeldKeys::new();
        down(A, &mut held, false);

        decide(KeyTransition::Up, A, true, true, false, false, &mut held);

        assert!(held.contains(A));
    }

    #[test]
    fn modifier_in_the_held_set_marks_the_next_key() {
        let mut held = HeldKeys::new();
        let (_, shift_report) = down(SHIFT, &mut held, false);
        assert_eq!(
            Some(CaptureReport {
                modifiers_held: false
            }),
            shift_report
        );

        let (_, report) = down(A, &mut held, false);

        assert_eq!(
            Some(CaptureReport {
                modifiers_held: true
            }),
            report
        );
    }

    #[test]
    fn async_modifier_state_marks_the_key() {
        let mut held = HeldKeys::new();

        let (_, report) = decide(KeyTransition::Down, A, false, true, false, true, &mut held);

        assert_eq!(
            Some(CaptureReport {
                modifiers_held: true
            }),
            report
        );
    }

    #[test]
    fn released_modifier_no_longer_marks_keys() {
        let mut held = HeldKeys::new();
        down(0xA0, &mut held, false);
        up(0xA0, &mut held);

        let (_, report) = down(A, &mut held, false);

        assert_eq!(
            Some(CaptureReport {
                modifiers_held: false
            }),
            report
        );
    }

    #[test]
    fn out_of_range_virtual_keys_do_not_panic() {
        let mut held = HeldKeys::new();

        let (action, _) = down(0x1FF, &mut held, false);

        assert_eq!(CaptureAction::Suppress, action);
        assert!(!held.contains(0x1FF));
    }
}
