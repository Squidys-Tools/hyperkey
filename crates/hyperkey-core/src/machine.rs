use crate::keys::{KeyTransition, KeyboardEvent, TriggerKey};
use crate::settings::OutputModifier;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TriggerPhase {
    Idle,
    TriggerHeld,
    HyperActive,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TriggerMachineState {
    pub phase: TriggerPhase,
}

impl TriggerMachineState {
    pub const IDLE: Self = Self {
        phase: TriggerPhase::Idle,
    };
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum InputDecision {
    PassThrough,
    Suppress,
    Forward,
    PressAndForward(Vec<OutputModifier>),
    ReleaseAndSuppress(Vec<OutputModifier>),
    ReplayTrigger,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TriggerTransition {
    pub state: TriggerMachineState,
    pub decision: InputDecision,
}

/// Equivalent to `process_with_trigger` with `TriggerKey::CAPS_LOCK`.
pub fn process(
    current: TriggerMachineState,
    input: &KeyboardEvent,
    output_modifiers: &[OutputModifier],
) -> TriggerTransition {
    process_with_trigger(current, input, TriggerKey::CAPS_LOCK, output_modifiers)
}

pub fn process_with_trigger(
    current: TriggerMachineState,
    input: &KeyboardEvent,
    trigger_key: TriggerKey,
    output_modifiers: &[OutputModifier],
) -> TriggerTransition {
    if input.is_synthetic {
        return TriggerTransition {
            state: current,
            decision: InputDecision::PassThrough,
        };
    }

    let is_trigger = input.key == trigger_key.to_virtual_key();
    match current.phase {
        TriggerPhase::Idle => process_idle(input, is_trigger),
        TriggerPhase::TriggerHeld => process_trigger_held(input, is_trigger, output_modifiers),
        TriggerPhase::HyperActive => process_hyper_active(input, is_trigger, output_modifiers),
    }
}

fn process_idle(input: &KeyboardEvent, is_trigger: bool) -> TriggerTransition {
    if is_trigger && input.transition == KeyTransition::Down {
        return TriggerTransition {
            state: TriggerMachineState {
                phase: TriggerPhase::TriggerHeld,
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
    input: &KeyboardEvent,
    is_trigger: bool,
    output_modifiers: &[OutputModifier],
) -> TriggerTransition {
    if is_trigger && input.transition == KeyTransition::Up {
        return TriggerTransition {
            state: TriggerMachineState::IDLE,
            decision: InputDecision::ReplayTrigger,
        };
    }

    if is_trigger {
        return TriggerTransition {
            state: TriggerMachineState {
                phase: TriggerPhase::TriggerHeld,
            },
            decision: InputDecision::Suppress,
        };
    }

    if input.transition == KeyTransition::Down {
        return TriggerTransition {
            state: TriggerMachineState {
                phase: TriggerPhase::HyperActive,
            },
            decision: InputDecision::PressAndForward(output_modifiers.to_vec()),
        };
    }

    TriggerTransition {
        state: TriggerMachineState {
            phase: TriggerPhase::TriggerHeld,
        },
        decision: InputDecision::PassThrough,
    }
}

fn process_hyper_active(
    input: &KeyboardEvent,
    is_trigger: bool,
    output_modifiers: &[OutputModifier],
) -> TriggerTransition {
    if is_trigger && input.transition == KeyTransition::Up {
        return TriggerTransition {
            state: TriggerMachineState::IDLE,
            decision: InputDecision::ReleaseAndSuppress(output_modifiers.to_vec()),
        };
    }

    if is_trigger {
        return TriggerTransition {
            state: TriggerMachineState {
                phase: TriggerPhase::HyperActive,
            },
            decision: InputDecision::Suppress,
        };
    }

    TriggerTransition {
        state: TriggerMachineState {
            phase: TriggerPhase::HyperActive,
        },
        decision: InputDecision::Forward,
    }
}
