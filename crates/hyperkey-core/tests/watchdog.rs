//! Ported from the C# hook-watchdog tests.

use hyperkey_core::watchdog::{HookWatchdog, HookWatchdogAction};

const DEAD_AFTER: u64 = 100;

#[test]
fn probes_while_heartbeats_arrive() {
    let watchdog = HookWatchdog::new(DEAD_AFTER, 0);

    assert_eq!(HookWatchdogAction::SendProbe, watchdog.tick(10));
    watchdog.heartbeat(10);
    assert_eq!(HookWatchdogAction::SendProbe, watchdog.tick(50));
    watchdog.heartbeat(50);
    assert_eq!(HookWatchdogAction::SendProbe, watchdog.tick(149));
}

#[test]
fn dead_hook_requests_reinstall_until_heartbeat_resumes() {
    let watchdog = HookWatchdog::new(250, 0);

    assert_eq!(HookWatchdogAction::SendProbe, watchdog.tick(100));
    assert_eq!(HookWatchdogAction::ReinstallHook, watchdog.tick(250));
    assert_eq!(HookWatchdogAction::ReinstallHook, watchdog.tick(500));
    watchdog.heartbeat(600);
    assert_eq!(HookWatchdogAction::SendProbe, watchdog.tick(700));
}

#[test]
fn reset_resumes_probing_after_reinstall() {
    let watchdog = HookWatchdog::new(DEAD_AFTER, 0);

    assert_eq!(HookWatchdogAction::ReinstallHook, watchdog.tick(100));
    watchdog.reset(200);
    assert_eq!(HookWatchdogAction::SendProbe, watchdog.tick(250));
    assert_eq!(HookWatchdogAction::ReinstallHook, watchdog.tick(300));
}

#[test]
fn heartbeat_boundary_is_exact() {
    let watchdog = HookWatchdog::new(DEAD_AFTER, 0);

    assert_eq!(HookWatchdogAction::SendProbe, watchdog.tick(99));
    watchdog.heartbeat(50);
    assert_eq!(HookWatchdogAction::SendProbe, watchdog.tick(149));
    assert_eq!(HookWatchdogAction::ReinstallHook, watchdog.tick(150));
}

#[test]
fn ticks_earlier_than_the_last_heartbeat_still_probe() {
    // The age computation saturates: a tick that reads earlier than the
    // last heartbeat — a wrapped or regressed tick source — must not look
    // like a dead hook and trigger a reinstall storm.
    let watchdog = HookWatchdog::new(DEAD_AFTER, 0);
    watchdog.heartbeat(500);

    assert_eq!(HookWatchdogAction::SendProbe, watchdog.tick(400));
    assert_eq!(HookWatchdogAction::SendProbe, watchdog.tick(0));
}
