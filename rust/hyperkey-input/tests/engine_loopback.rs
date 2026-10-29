//! End-to-end coverage for the generalized trigger against the real Windows
//! input stack: a live `InputEngine` plus a second low-level hook that observes
//! exactly what survives the engine. The trigger under test is F22 and the
//! chord key is F23 — both inert everywhere, so the test types nothing
//! observable no matter which window has focus. Uses only process-local hooks,
//! SendInput, and async key state, so it needs no window, no focus, and no
//! text field.
//!
//! Runs only on demand: `cargo test -p hyperkey-input -- --ignored`.

use hyperkey_core::keys::{KeyTransition, TriggerKey};
use hyperkey_core::settings::OutputModifier;
use hyperkey_input::engine::InputEngine;
use hyperkey_input::synth::SYNTHETIC_INPUT_TAG;
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, PM_REMOVE, PeekMessageW,
    PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

const TRIGGER_CODE: u16 = 0x85; // F22.
const CHORD_CODE: u16 = 0x86; // F23.

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
static DONE: AtomicBool = AtomicBool::new(false);
static MAIN_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static OBSERVER_HOOK: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

#[test]
#[ignore]
fn generalized_trigger_works_end_to_end_on_the_real_hook() {
    let trigger = TriggerKey::try_create(TRIGGER_CODE)
        .expect("F22 must be a supported trigger for the loopback test.");
    let modifiers = vec![
        OutputModifier::Control,
        OutputModifier::Alt,
        OutputModifier::Shift,
    ];
    let engine = InputEngine::new(trigger, modifiers);

    {
        let mut log = LOG.lock().unwrap();
        log.clear();
        DONE.store(false, Ordering::SeqCst);
    }

    // Windows calls low-level hooks in reverse installation order (most
    // recently installed runs first), so the observation hook is installed
    // BEFORE the engine starts: the engine then runs first, and anything it
    // suppresses never reaches this observer.
    // SAFETY: FFI calls; the hook is installed on this test thread and pumped
    // by pump_messages below; the module handle is this test binary.
    MAIN_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
    let module = unsafe { GetModuleHandleW(None) };
    let observer = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(observer_hook_callback),
            module.ok().map(|module| HINSTANCE(module.0)),
            0,
        )
    };
    let observer = observer.expect("The test observation hook could not be installed.");
    OBSERVER_HOOK.store(observer.0, Ordering::SeqCst);

    let run_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), String> {
        if !engine.start() {
            return Err("The engine under test could not be started.".to_string());
        }

        let sender = std::thread::Builder::new()
            .name("loopback sender".to_string())
            .spawn(sender_main)
            .map_err(|error| format!("The loopback sender could not start. {error}"))?;
        pump_messages(Duration::from_secs(25))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !sender.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if !sender.is_finished() {
            return Err("The loopback sender thread did not finish.".to_string());
        }
        let _ = sender.join();

        assert_log_sequence()
    }));
    let failure = match run_result {
        Ok(result) => result.err(),
        Err(payload) => Some(match payload.downcast::<String>() {
            Ok(message) => *message,
            Err(payload) => payload
                .downcast::<&str>()
                .map(|message| (*message).to_string())
                .unwrap_or_else(|_| "The loopback test panicked.".to_string()),
        }),
    };

    engine.stop();
    // SAFETY: FFI cleanup for the observer hook this thread installed.
    let _ = unsafe { UnhookWindowsHookEx(HHOOK(OBSERVER_HOOK.swap(std::ptr::null_mut(), Ordering::SeqCst))) };
    release_all_modifiers();
    drop(engine);

    if let Some(failure) = failure {
        panic!("{failure}");
    }
}

fn sender_main() {
    std::thread::sleep(Duration::from_millis(800));

    // Tap: raw events must vanish, replayed (tagged) pair must arrive.
    send_key(TRIGGER_CODE, false);
    std::thread::sleep(Duration::from_millis(350));
    send_key(TRIGGER_CODE, true);
    std::thread::sleep(Duration::from_millis(400));

    // Chord: trigger held while the chord key goes down and up.
    send_key(TRIGGER_CODE, false);
    std::thread::sleep(Duration::from_millis(350));
    send_key(CHORD_CODE, false);
    std::thread::sleep(Duration::from_millis(350));
    record_async_state("mid");
    send_key(CHORD_CODE, true);
    std::thread::sleep(Duration::from_millis(250));
    send_key(TRIGGER_CODE, true);
    std::thread::sleep(Duration::from_millis(400));
    record_async_state("post");

    DONE.store(true, Ordering::SeqCst);
    // SAFETY: FFI call; wakes the pumping test thread.
    let _ = unsafe {
        PostThreadMessageW(
            MAIN_THREAD_ID.load(Ordering::SeqCst),
            WM_QUIT,
            WPARAM(0),
            LPARAM(0),
        )
    };
}

fn record_async_state(phase: &str) {
    // SAFETY: FFI query with no preconditions.
    let control = unsafe { GetAsyncKeyState(0x11) } as u16 & 0x8000 != 0;
    let alt = unsafe { GetAsyncKeyState(0x12) } as u16 & 0x8000 != 0;
    let shift = unsafe { GetAsyncKeyState(0x10) } as u16 & 0x8000 != 0;
    let flag = |value: bool| if value { "True" } else { "False" };
    log_line(format!(
        "ASYNC-{phase}:Ctrl={}:Alt={}:Shift={}",
        flag(control),
        flag(alt),
        flag(shift)
    ));
}

fn assert_log_sequence() -> Result<(), String> {
    let log = LOG.lock().unwrap().clone();
    let require = |condition: bool, message: &str| -> Result<(), String> {
        if condition {
            Ok(())
        } else {
            Err(format!("{message}\nObserved: {}", log.join(" | ")))
        }
    };

    // Raw trigger events must never survive the engine.
    let raw_down = format!("EVT:DOWN:0x{TRIGGER_CODE:02X}:R");
    let raw_up = format!("EVT:UP:0x{TRIGGER_CODE:02X}:R");
    require(
        !log.iter().any(|line| line == &raw_down),
        "Raw F22 key-down reached the observer; the trigger was not suppressed.",
    )?;
    require(
        !log.iter().any(|line| line == &raw_up),
        "Raw F22 key-up reached the observer; the trigger was not suppressed.",
    )?;

    // The tap must come back as a replay pair carrying this process's
    // synthetic tag, in order.
    let own_tag = describe_tag(SYNTHETIC_INPUT_TAG as usize);
    let tagged_down = format!("EVT:DOWN:0x{TRIGGER_CODE:02X}:{own_tag}");
    let tagged_up = format!("EVT:UP:0x{TRIGGER_CODE:02X}:{own_tag}");
    let replay_down = log.iter().position(|line| line == &tagged_down);
    let replay_up = log.iter().position(|line| line == &tagged_up);
    require(
        replay_down.is_some() && replay_up == replay_down.map(|index| index + 1),
        "Expected a tagged F22 down/up replay pair for the tap.",
    )?;

    // The chord key travels through untouched while the trigger is held.
    // (Modifier presses interleave asynchronously, so only order matters.)
    let chord_down = log
        .iter()
        .position(|line| line == &format!("EVT:DOWN:0x{CHORD_CODE:02X}:R"));
    let chord_up = log
        .iter()
        .position(|line| line == &format!("EVT:UP:0x{CHORD_CODE:02X}:R"));
    require(
        chord_down.is_some() && chord_up > chord_down,
        "Expected the raw F23 down/up pair to be forwarded during the chord.",
    )?;
    require(
        chord_down > replay_up,
        "Expected the chord to happen after the tap replay.",
    )?;

    // Output modifiers are physically held mid-chord and released after.
    let mid = log.iter().find(|line| line.starts_with("ASYNC-mid:"));
    let post = log.iter().find(|line| line.starts_with("ASYNC-post:"));
    require(
        mid.map(String::as_str) == Some("ASYNC-mid:Ctrl=True:Alt=True:Shift=True"),
        "Expected Ctrl+Alt+Shift to be physically held mid-chord.",
    )?;
    require(
        post.map(String::as_str) == Some("ASYNC-post:Ctrl=False:Alt=False:Shift=False"),
        "Expected all output modifiers to be released after the chord.",
    )
}

fn pump_messages(timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let mut drained = false;
        let mut message = MSG::default();
        // SAFETY: FFI message pump on this test thread's queue.
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            drained = true;
            if message.message == WM_QUIT {
                return Ok(());
            }

            // SAFETY: FFI dispatch for the message just retrieved.
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            message = MSG::default();
        }

        if DONE.load(Ordering::SeqCst) && !drained {
            return Ok(());
        }

        std::thread::sleep(Duration::from_millis(20));
    }

    Err("Timed out waiting for the loopback sender.".to_string())
}

unsafe extern "system" fn observer_hook_callback(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0
        && let Some(transition) = try_get_transition(wparam.0 as u32)
    {
        // SAFETY: Windows hands the callback a valid KBDLLHOOKSTRUCT.
        let native = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
        log_line(format!(
            "EVT:{}:0x{:02X}:{}",
            if transition == KeyTransition::Down {
                "DOWN"
            } else {
                "UP"
            },
            native.vkCode as u16,
            describe_tag(native.dwExtraInfo)
        ));
    }

    // SAFETY: FFI call; unhandled events must always be chained.
    unsafe { CallNextHookEx(Some(observer()), code, wparam, lparam) }
}

fn observer() -> HHOOK {
    HHOOK(OBSERVER_HOOK.load(Ordering::SeqCst))
}

/// Renders the raw `dwExtraInfo` marker so any injector is identifiable in
/// the log: "R" for untagged physical input, "T:" plus the marker for
/// injected events. Markers that decode as printable ASCII (this process's
/// own "HYPERKEY", foreign markers like "HYPERKHB") are shown as text;
/// anything else is shown as hex.
fn describe_tag(extra_info: usize) -> String {
    if extra_info == 0 {
        return "R".to_string();
    }

    let bytes = (extra_info as u64).to_be_bytes();
    if bytes.iter().all(u8::is_ascii_graphic) {
        format!("T:{}", String::from_utf8_lossy(&bytes))
    } else {
        format!("T:0x{:016X}", extra_info)
    }
}

fn try_get_transition(message: u32) -> Option<KeyTransition> {
    match message {
        WM_KEYDOWN | WM_SYSKEYDOWN => Some(KeyTransition::Down),
        WM_KEYUP | WM_SYSKEYUP => Some(KeyTransition::Up),
        _ => None,
    }
}

fn send_key(virtual_key: u16, is_key_up: bool) {
    let inputs = [INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(virtual_key),
                wScan: 0,
                dwFlags: if is_key_up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }];

    // SAFETY: FFI call; the slice is a valid INPUT array for the duration.
    if unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) } != inputs.len() as u32 {
        log_line(format!(
            "SEND-FAILED:0x{virtual_key:02X}:{}",
            if is_key_up { "UP" } else { "DOWN" }
        ));
    }
}

fn release_all_modifiers() {
    for code in [0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5] {
        send_key(code, true);
    }
}

fn log_line(line: String) {
    LOG.lock().unwrap().push(line);
}
