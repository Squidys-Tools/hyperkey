//! Pure, time-based decision logic for low-level keyboard hook self-healing.
//! No Win32: the caller supplies millisecond ticks (the engine uses
//! `GetTickCount64`, the tests supply constants).
//!
//! Ported from the C# hook watchdog, removed with the C# app. The hook callback stamps a
//! heartbeat on every event it sees; a timer-driven probe (a tagged SendInput
//! key-up) keeps beats arriving while the system is otherwise quiet. Windows
//! silently removes low-level hooks that stall, so a heartbeat older than the
//! dead-after threshold is treated as a dead hook and a reinstall is requested
//! on every tick until a fresh heartbeat or [`HookWatchdog::reset`] proves the
//! hook is back.

use std::sync::atomic::{AtomicU64, Ordering};

/// What a watchdog tick asks the engine to do next.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum HookWatchdogAction {
    /// Heartbeats are fresh; inject a probe so quiet periods still produce a beat.
    SendProbe,
    /// The heartbeat is stale; the hook is presumed silently removed.
    ReinstallHook,
}

/// Tracks the last heartbeat tick and decides between probing and reinstalling.
/// The heartbeat field is atomic because the hook callback writes it while the
/// watchdog thread reads it.
pub struct HookWatchdog {
    dead_after_milliseconds: u64,
    last_heartbeat_ticks: AtomicU64,
}

impl HookWatchdog {
    pub fn new(dead_after_milliseconds: u64, start_ticks: u64) -> Self {
        Self {
            dead_after_milliseconds,
            last_heartbeat_ticks: AtomicU64::new(start_ticks),
        }
    }

    /// Stamps a heartbeat proving the hook still sees input.
    pub fn heartbeat(&self, now_ticks: u64) {
        self.last_heartbeat_ticks.store(now_ticks, Ordering::SeqCst);
    }

    /// Called after a successful reinstall; identical to `heartbeat`.
    pub fn reset(&self, now_ticks: u64) {
        self.heartbeat(now_ticks);
    }

    /// `SendProbe` while the last heartbeat is younger than the dead-after
    /// threshold; `ReinstallHook` once it goes stale. `saturating_sub` keeps a
    /// regressed or wrapped tick source from reading as a huge age.
    pub fn tick(&self, now_ticks: u64) -> HookWatchdogAction {
        if now_ticks.saturating_sub(self.last_heartbeat_ticks.load(Ordering::SeqCst))
            >= self.dead_after_milliseconds
        {
            HookWatchdogAction::ReinstallHook
        } else {
            HookWatchdogAction::SendProbe
        }
    }
}
