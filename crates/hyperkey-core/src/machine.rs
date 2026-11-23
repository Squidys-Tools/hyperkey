//! The pure trigger state machine.
//!
//! Every event carries a `time_ms` tick (the hook's `KBDLLHOOKSTRUCT.time`),
//! and every duration is computed with `wrapping_sub`, so the machine needs
//! no clock of its own. The one decision that can come due with no event —
//! the permissive-hold timeout — is exposed as [`TriggerMachineState::timer_deadline`]
//! plus the pure [`on_timeout`] transition; the engine arms a timer for the
//! deadline and calls `on_timeout` when it fires.
//!
//! # Permissive hold
//!
//! A trigger that is also a typing key ([`TriggerKey::is_typing_key`]) is
//! undecided while held: the next non-modifier key-downs are suppressed and
//! buffered until one of these resolves it:
//!
//! - a buffered key's key-up arrives while the trigger is still held: **hold**
//!   (press the modifiers, replay the buffered downs, then the up);
//! - the trigger is released first: **tap** (rollover typing — perform the tap
//!   action, then replay the buffered downs without modifiers);
//! - [`HOLD_TIMEOUT_MS`] passes since the trigger went down: **hold**;
//! - the buffer overflows [`MAX_BUFFERED_KEYS`]: **hold**.
//!
//! Other triggers activate the layer on the first non-modifier key-down.
//! Modifier keys (Shift, Ctrl, Alt, Win) never change the phase. They pass
//! through at once, except while an undecided hold has keys buffered: then
//! they are buffered too, so the replay delivers every key with the modifier
//! state it was pressed under.
//!
//! An event stamped at or past the hold deadline means the timer is merely
//! late. The engine checks [`TriggerMachineState::deadline_passed`] and runs
//! [`on_timeout`] before [`process_event`], so the overdue timeout resolves
//! first and the outcome depends on event times, not on
//! message-queue latency.

use crate::keys::{KeyTransition, KeyboardEvent, TriggerKey, VirtualKey, is_modifier_key};
use crate::settings::{OutputModifier, TapBehavior};

/// A trigger held alone for longer than this is released without a tap: the
/// user changed their mind about a chord.
pub const TAP_TIMEOUT_MS: u32 = 500;

/// A typing-key trigger held this long is decided as a hold.
pub const HOLD_TIMEOUT_MS: u32 = 200;

/// The most non-modifier key-downs buffered while a typing-key trigger is
/// undecided; one more resolves the trigger as a hold. Buffered modifier
/// events do not count.
pub const MAX_BUFFERED_KEYS: usize = 8;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TriggerPhase {
    Idle,
    TriggerHeld,
    HyperActive,
}

/// A suppressed key event the engine must replay later, with everything
/// needed to synthesize it again through `SendInput`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct BufferedKey {
    pub key: VirtualKey,
    pub scan_code: u16,
    pub extended: bool,
    pub transition: KeyTransition,
}

impl BufferedKey {
    pub fn from_event(event: &KeyboardEvent) -> Self {
        Self {
            key: event.key,
            scan_code: event.scan_code,
            extended: event.extended,
            transition: event.transition,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TriggerMachineState {
    pub phase: TriggerPhase,
    /// Tick of the trigger key-down that began the current hold.
    pub trigger_down_ms: u32,
    /// Tick at which an undecided typing-key hold resolves as a hold. `None`
    /// outside `TriggerHeld`, for non-typing triggers, and once decided.
    pub hold_deadline_ms: Option<u32>,
    /// True once the hold timed out with nothing buffered: the next
    /// non-modifier key-down activates the layer at once.
    pub hold_decided: bool,
    /// Events suppressed while the hold is undecided, in arrival order: the
    /// non-modifier key-downs (and the up that resolves a chord) plus any
    /// modifier events that arrived after the first of them.
    pub buffered: Vec<BufferedKey>,
}

impl TriggerMachineState {
    pub const IDLE: Self = Self {
        phase: TriggerPhase::Idle,
        trigger_down_ms: 0,
        hold_deadline_ms: None,
        hold_decided: false,
        buffered: Vec::new(),
    };

    /// A state in `phase` with no hold bookkeeping; convenient for tests and
    /// for resetting.
    pub fn in_phase(phase: TriggerPhase) -> Self {
        Self {
            phase,
            ..Self::IDLE
        }
    }

    /// The tick at which [`on_timeout`] must run, if a timer should be armed.
    pub fn timer_deadline(&self) -> Option<u32> {
        match self.phase {
            TriggerPhase::TriggerHeld => self.hold_deadline_ms,
            _ => None,
        }
    }

    /// True when a timer deadline is armed and tick `now_ms` is at or past
    /// it: [`on_timeout`] is overdue and must run before the next event.
    pub fn deadline_passed(&self, now_ms: u32) -> bool {
        self.timer_deadline()
            .is_some_and(|deadline| reached(now_ms, deadline))
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum InputDecision {
    /// Let the event through; no state of interest.
    PassThrough,
    /// Swallow the event.
    Suppress,
    /// Let the event through while the layer is active.
    Forward,
    /// Press the modifiers, then let the event through.
    PressAndForward(Vec<OutputModifier>),
    /// Release the modifiers and swallow the event.
    ReleaseAndSuppress(Vec<OutputModifier>),
    /// Swallow the event, perform the tap action, then replay `then_replay`
    /// with no modifiers held.
    Tap {
        action: TapBehavior,
        then_replay: Vec<BufferedKey>,
    },
    /// Swallow the event (if any), press the modifiers, then replay `keys`
    /// in order. The layer is active afterwards.
    PressAndReplay {
        modifiers: Vec<OutputModifier>,
        keys: Vec<BufferedKey>,
    },
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TriggerTransition {
    pub state: TriggerMachineState,
    pub decision: InputDecision,
}

/// Everything the machine needs besides its state and the event.
#[derive(Copy, Clone, Debug)]
pub struct MachineConfig<'a> {
    pub trigger: TriggerKey,
    pub output_modifiers: &'a [OutputModifier],
    pub tap_behavior: TapBehavior,
}

/// Equivalent to `process_with_trigger` with `TriggerKey::CAPS_LOCK`.
pub fn process(
    current: TriggerMachineState,
    input: &KeyboardEvent,
    output_modifiers: &[OutputModifier],
) -> TriggerTransition {
    process_with_trigger(current, input, TriggerKey::CAPS_LOCK, output_modifiers)
}

/// Equivalent to `process_event` with the default tap behavior.
pub fn process_with_trigger(
    current: TriggerMachineState,
    input: &KeyboardEvent,
    trigger_key: TriggerKey,
    output_modifiers: &[OutputModifier],
) -> TriggerTransition {
    process_event(
        current,
        input,
        &MachineConfig {
            trigger: trigger_key,
            output_modifiers,
            tap_behavior: TapBehavior::default(),
        },
    )
}

pub fn process_event(
    mut current: TriggerMachineState,
    input: &KeyboardEvent,
    config: &MachineConfig<'_>,
) -> TriggerTransition {
    if input.is_synthetic {
        return stay(current, InputDecision::PassThrough);
    }

    let is_trigger = input.key == config.trigger.to_virtual_key();
    if !is_trigger && is_modifier_key(input.key.0) {
        // A modifier that follows a buffered key must reach the OS after
        // that key, or the replay types it under the wrong modifier state.
        if current.phase == TriggerPhase::TriggerHeld && !current.buffered.is_empty() {
            current.buffered.push(BufferedKey::from_event(input));
            return stay(current, InputDecision::Suppress);
        }
        // Real modifiers never change the phase, so Hyper+Shift chords and
        // the user's own modifiers keep working in every phase.
        let decision = if current.phase == TriggerPhase::HyperActive {
            InputDecision::Forward
        } else {
            InputDecision::PassThrough
        };
        return stay(current, decision);
    }

    match current.phase {
        TriggerPhase::Idle => process_idle(input, is_trigger, config),
        TriggerPhase::TriggerHeld => process_trigger_held(current, input, is_trigger, config),
        TriggerPhase::HyperActive => process_hyper_active(current, input, is_trigger, config),
    }
}

/// The permissive-hold timer fired at `now_ms`. Resolves an undecided hold
/// whose deadline has been reached; otherwise the state is unchanged. The
/// decision is `PressAndReplay` when buffered keys must be replayed and
/// `Suppress` (nothing to do) otherwise.
pub fn on_timeout(
    current: TriggerMachineState,
    now_ms: u32,
    config: &MachineConfig<'_>,
) -> TriggerTransition {
    match current.timer_deadline() {
        Some(deadline) if reached(now_ms, deadline) => resolve_hold_timeout(current, config),
        _ => stay(current, InputDecision::Suppress),
    }
}

fn resolve_hold_timeout(
    mut current: TriggerMachineState,
    config: &MachineConfig<'_>,
) -> TriggerTransition {
    current.hold_deadline_ms = None;
    if current.buffered.is_empty() {
        current.hold_decided = true;
        return stay(current, InputDecision::Suppress);
    }

    let keys = std::mem::take(&mut current.buffered);
    current.phase = TriggerPhase::HyperActive;
    stay(
        current,
        InputDecision::PressAndReplay {
            modifiers: config.output_modifiers.to_vec(),
            keys,
        },
    )
}

fn process_idle(
    input: &KeyboardEvent,
    is_trigger: bool,
    config: &MachineConfig<'_>,
) -> TriggerTransition {
    if is_trigger && input.transition == KeyTransition::Down {
        let hold_deadline_ms = config
            .trigger
            .is_typing_key()
            .then(|| input.time_ms.wrapping_add(HOLD_TIMEOUT_MS));
        return TriggerTransition {
            state: TriggerMachineState {
                phase: TriggerPhase::TriggerHeld,
                trigger_down_ms: input.time_ms,
                hold_deadline_ms,
                hold_decided: false,
                buffered: Vec::new(),
            },
            decision: InputDecision::Suppress,
        };
    }

    TriggerTransition {
        state: TriggerMachineState::IDLE,
        decision: InputDecision::PassThrough,
    }
}

fn process_trigger_held(
    mut current: TriggerMachineState,
    input: &KeyboardEvent,
    is_trigger: bool,
    config: &MachineConfig<'_>,
) -> TriggerTransition {
    // An event stamped past an empty hold's deadline means the timer simply
    // has not been dispatched yet; decide the hold now so the outcome
    // depends on event times, not on message-queue latency.
    if let Some(deadline) = current.hold_deadline_ms
        && current.buffered.is_empty()
        && reached(input.time_ms, deadline)
    {
        current.hold_deadline_ms = None;
        current.hold_decided = true;
    }

    if is_trigger {
        if input.transition == KeyTransition::Down {
            // Auto-repeat while held.
            return stay(current, InputDecision::Suppress);
        }

        let buffered = std::mem::take(&mut current.buffered);
        let held_ms = input.time_ms.wrapping_sub(current.trigger_down_ms);
        // A rollover is always a tap; a lone long hold is a cancelled chord.
        let decision = if buffered.is_empty() && held_ms > TAP_TIMEOUT_MS {
            InputDecision::Suppress
        } else {
            InputDecision::Tap {
                action: config.tap_behavior,
                then_replay: buffered,
            }
        };
        return TriggerTransition {
            state: TriggerMachineState::IDLE,
            decision,
        };
    }

    let undecided = current.hold_deadline_ms.is_some() && !current.hold_decided;

    match input.transition {
        KeyTransition::Down if undecided => {
            current.buffered.push(BufferedKey::from_event(input));
            let buffered_keys = current
                .buffered
                .iter()
                .filter(|key| !is_modifier_key(key.key.0))
                .count();
            if buffered_keys > MAX_BUFFERED_KEYS {
                return resolve_hold_with(current, config);
            }
            stay(current, InputDecision::Suppress)
        }
        KeyTransition::Down => {
            current.phase = TriggerPhase::HyperActive;
            current.hold_deadline_ms = None;
            current.buffered.clear();
            stay(
                current,
                InputDecision::PressAndForward(config.output_modifiers.to_vec()),
            )
        }
        KeyTransition::Up if current.buffered.iter().any(|key| key.key == input.key) => {
            // A full press of a key inside the hold: it was a chord.
            current.buffered.push(BufferedKey::from_event(input));
            resolve_hold_with(current, config)
        }
        // A key pressed before the trigger is being released.
        KeyTransition::Up => stay(current, InputDecision::PassThrough),
    }
}

/// Resolves an undecided hold as a hold, replaying everything buffered.
fn resolve_hold_with(
    mut current: TriggerMachineState,
    config: &MachineConfig<'_>,
) -> TriggerTransition {
    let keys = std::mem::take(&mut current.buffered);
    current.phase = TriggerPhase::HyperActive;
    current.hold_deadline_ms = None;
    stay(
        current,
        InputDecision::PressAndReplay {
            modifiers: config.output_modifiers.to_vec(),
            keys,
        },
    )
}

fn process_hyper_active(
    current: TriggerMachineState,
    input: &KeyboardEvent,
    is_trigger: bool,
    config: &MachineConfig<'_>,
) -> TriggerTransition {
    if is_trigger && input.transition == KeyTransition::Up {
        return TriggerTransition {
            state: TriggerMachineState::IDLE,
            decision: InputDecision::ReleaseAndSuppress(config.output_modifiers.to_vec()),
        };
    }

    if is_trigger {
        return stay(current, InputDecision::Suppress);
    }

    stay(current, InputDecision::Forward)
}

fn stay(state: TriggerMachineState, decision: InputDecision) -> TriggerTransition {
    TriggerTransition { state, decision }
}

/// True when tick `now` is at or past tick `deadline`, across wraparound.
fn reached(now: u32, deadline: u32) -> bool {
    (now.wrapping_sub(deadline) as i32) >= 0
}
