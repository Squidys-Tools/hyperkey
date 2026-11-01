//! Port of tests/Hyperkey.Core.Tests/HookWatchdogTests.cs.

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
