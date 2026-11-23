//! Permissive hold for typing-key triggers, the long-hold tap cancel, tap
//! actions, and modifier pass-through. Every event carries an explicit tick
//! so timeouts are exercised without a clock.

use hyperkey_core::keys::{KeyTransition, KeyboardEvent, TriggerKey, VirtualKey};
use hyperkey_core::machine::{
    self, BufferedKey, HOLD_TIMEOUT_MS, InputDecision, MAX_BUFFERED_KEYS, MachineConfig,
    TAP_TIMEOUT_MS, TriggerMachineState, TriggerPhase, TriggerTransition,
};
use hyperkey_core::settings::{OutputModifier, TapBehavior};

const SPACE: u16 = 0x20;
const F: u16 = 0x46;
const J: u16 = 0x4A;
const A: u16 = 0x41;
const LSHIFT: u16 = 0xA0;

static MODS: &[OutputModifier] = &[
    OutputModifier::Control,
    OutputModifier::Alt,
    OutputModifier::Shift,
];

fn config(trigger: u16, tap_behavior: TapBehavior) -> MachineConfig<'static> {
    MachineConfig {
        trigger: TriggerKey::try_create(trigger).expect("supported trigger"),
        output_modifiers: MODS,
        tap_behavior,
    }
}

fn space() -> MachineConfig<'static> {
    config(SPACE, TapBehavior::SendTrigger)
}

fn down(code: u16, time: u32) -> KeyboardEvent {
    KeyboardEvent::new(VirtualKey(code), KeyTransition::Down)
        .at(time)
        .with_scan_code(code + 0x100, code == F)
}

fn up(code: u16, time: u32) -> KeyboardEvent {
    KeyboardEvent::new(VirtualKey(code), KeyTransition::Up)
        .at(time)
        .with_scan_code(code + 0x100, code == F)
}

fn buffered(code: u16, transition: KeyTransition) -> BufferedKey {
    BufferedKey {
        key: VirtualKey(code),
        scan_code: code + 0x100,
        extended: code == F,
        transition,
    }
}

/// Feeds events in order, returning every transition.
fn run(
    config: &MachineConfig<'_>,
    events: &[KeyboardEvent],
) -> (TriggerMachineState, Vec<InputDecision>) {
    let mut state = TriggerMachineState::IDLE;
    let mut decisions = Vec::new();
    for event in events {
        let TriggerTransition {
            state: next,
            decision,
        } = machine::process_event(state, event, config);
        state = next;
        decisions.push(decision);
    }
    (state, decisions)
}

fn send_trigger_tap(then_replay: Vec<BufferedKey>) -> InputDecision {
    InputDecision::Tap {
        action: TapBehavior::SendTrigger,
        then_replay,
    }
}

fn press_and_replay(keys: Vec<BufferedKey>) -> InputDecision {
    InputDecision::PressAndReplay {
        modifiers: MODS.to_vec(),
        keys,
    }
}

#[test]
fn typing_key_classification() {
    for code in [
        0x08, 0x09, 0x0D, 0x20, 0x30, 0x39, 0x41, 0x5A, 0x60, 0x6C, 0x6E, 0xBA, 0xC0, 0xDB, 0xDF,
    ] {
        let key = TriggerKey::try_create(code).unwrap();
        assert!(key.is_typing_key(), "0x{code:02X} should be a typing key");
    }
    for code in [0x14, 0x91, 0x90, 0x70, 0x7B, 0x87] {
        let key = TriggerKey::try_create(code).unwrap();
        assert!(
            !key.is_typing_key(),
            "0x{code:02X} should not be a typing key"
        );
    }
}

#[test]
fn typing_trigger_down_arms_the_hold_deadline() {
    let (state, decisions) = run(&space(), &[down(SPACE, 1_000)]);
    assert_eq!(vec![InputDecision::Suppress], decisions);
    assert_eq!(TriggerPhase::TriggerHeld, state.phase);
    assert_eq!(Some(1_000 + HOLD_TIMEOUT_MS), state.timer_deadline());
}

#[test]
fn non_typing_trigger_never_arms_a_timer() {
    let caps = config(0x14, TapBehavior::SendTrigger);
    let (state, _) = run(&caps, &[down(0x14, 1_000)]);
    assert_eq!(TriggerPhase::TriggerHeld, state.phase);
    assert_eq!(None, state.timer_deadline());
}

#[test]
fn rollover_typing_is_a_tap_followed_by_the_buffered_key() {
    // "space" then "f" typed fast: Space is still down when F goes down.
    let (state, decisions) = run(
        &space(),
        &[down(SPACE, 0), down(F, 40), up(SPACE, 80), up(F, 120)],
    );
    assert_eq!(
        vec![
            InputDecision::Suppress,
            InputDecision::Suppress,
            send_trigger_tap(vec![buffered(F, KeyTransition::Down)]),
            InputDecision::PassThrough,
        ],
        decisions
    );
    assert_eq!(TriggerMachineState::IDLE, state);
}

#[test]
fn rollover_of_several_keys_replays_them_in_order() {
    let (_, decisions) = run(
        &space(),
        &[down(SPACE, 0), down(F, 20), down(J, 40), up(SPACE, 60)],
    );
    assert_eq!(
        send_trigger_tap(vec![
            buffered(F, KeyTransition::Down),
            buffered(J, KeyTransition::Down),
        ]),
        decisions[3]
    );
}

#[test]
fn rollover_is_a_tap_even_after_the_long_hold_window() {
    // Keys were buffered, so the long-hold cancel does not apply. (The hold
    // timer would normally have fired; this checks the release rule alone.)
    let mut state =
        machine::process_event(TriggerMachineState::IDLE, &down(SPACE, 0), &space()).state;
    state = machine::process_event(state, &down(F, 10), &space()).state;
    let released = machine::process_event(state, &up(SPACE, TAP_TIMEOUT_MS + 100), &space());
    assert_eq!(
        send_trigger_tap(vec![buffered(F, KeyTransition::Down)]),
        released.decision
    );
}

#[test]
fn quick_full_chord_press_is_a_hold() {
    // Space held, F pressed and released before the hold timeout.
    let (state, decisions) = run(
        &space(),
        &[down(SPACE, 0), down(F, 30), up(F, 90), up(SPACE, 150)],
    );
    assert_eq!(
        vec![
            InputDecision::Suppress,
            InputDecision::Suppress,
            press_and_replay(vec![
                buffered(F, KeyTransition::Down),
                buffered(F, KeyTransition::Up),
            ]),
            InputDecision::ReleaseAndSuppress(MODS.to_vec()),
        ],
        decisions
    );
    assert_eq!(TriggerMachineState::IDLE, state);
}

#[test]
fn chord_key_held_past_the_timeout_resolves_as_a_hold() {
    let (state, _) = run(&space(), &[down(SPACE, 0), down(F, 50)]);
    assert_eq!(Some(HOLD_TIMEOUT_MS), state.timer_deadline());

    let fired = machine::on_timeout(state, HOLD_TIMEOUT_MS, &space());
    assert_eq!(
        press_and_replay(vec![buffered(F, KeyTransition::Down)]),
        fired.decision
    );
    assert_eq!(TriggerPhase::HyperActive, fired.state.phase);
    assert_eq!(None, fired.state.timer_deadline());
    assert!(fired.state.buffered.is_empty());

    // The physical F key-up and the trigger release follow the active layer.
    let f_up = machine::process_event(fired.state, &up(F, 260), &space());
    assert_eq!(InputDecision::Forward, f_up.decision);
    let released = machine::process_event(f_up.state, &up(SPACE, 300), &space());
    assert_eq!(
        InputDecision::ReleaseAndSuppress(MODS.to_vec()),
        released.decision
    );
}

#[test]
fn an_early_timer_changes_nothing() {
    let (state, _) = run(&space(), &[down(SPACE, 0), down(F, 50)]);
    let fired = machine::on_timeout(state.clone(), HOLD_TIMEOUT_MS - 1, &space());
    assert_eq!(InputDecision::Suppress, fired.decision);
    assert_eq!(state, fired.state);
}

#[test]
fn timeout_outside_a_hold_changes_nothing() {
    let fired = machine::on_timeout(TriggerMachineState::IDLE, 10_000, &space());
    assert_eq!(InputDecision::Suppress, fired.decision);
    assert_eq!(TriggerMachineState::IDLE, fired.state);
}

#[test]
fn timeout_with_nothing_buffered_decides_the_hold() {
    let (state, _) = run(&space(), &[down(SPACE, 0)]);
    let fired = machine::on_timeout(state, HOLD_TIMEOUT_MS + 5, &space());
    assert_eq!(InputDecision::Suppress, fired.decision);
    assert_eq!(TriggerPhase::TriggerHeld, fired.state.phase);
    assert!(fired.state.hold_decided);
    assert_eq!(None, fired.state.timer_deadline());

    // The next key goes straight to the layer, no buffering.
    let active = machine::process_event(fired.state, &down(F, 260), &space());
    assert_eq!(
        InputDecision::PressAndForward(MODS.to_vec()),
        active.decision
    );
    assert_eq!(TriggerPhase::HyperActive, active.state.phase);
}

#[test]
fn a_key_stamped_after_an_empty_deadline_needs_no_timer() {
    // The timer has not been dispatched yet, but the event time is past the
    // deadline, so the hold is already decided.
    let (_, decisions) = run(&space(), &[down(SPACE, 0), down(F, HOLD_TIMEOUT_MS + 1)]);
    assert_eq!(InputDecision::PressAndForward(MODS.to_vec()), decisions[1]);
}

#[test]
fn decided_hold_released_quickly_is_still_a_tap() {
    let (state, _) = run(&space(), &[down(SPACE, 0)]);
    let fired = machine::on_timeout(state, HOLD_TIMEOUT_MS, &space());
    let released = machine::process_event(fired.state, &up(SPACE, 300), &space());
    assert_eq!(send_trigger_tap(vec![]), released.decision);
}

#[test]
fn modifiers_are_ignored_in_every_phase() {
    // Idle.
    let idle = machine::process_event(TriggerMachineState::IDLE, &down(LSHIFT, 0), &space());
    assert_eq!(InputDecision::PassThrough, idle.decision);
    assert_eq!(TriggerPhase::Idle, idle.state.phase);

    // TriggerHeld (typing trigger): Shift never resolves the hold. It passes
    // through while nothing is buffered, and is buffered in order after a
    // buffered key so the replay keeps the original modifier state.
    let (state, decisions) = run(
        &space(),
        &[
            down(SPACE, 0),
            down(LSHIFT, 10),
            down(F, 20),
            up(LSHIFT, 30),
            up(F, 40),
        ],
    );
    assert_eq!(InputDecision::PassThrough, decisions[1]);
    assert_eq!(InputDecision::Suppress, decisions[2]);
    assert_eq!(InputDecision::Suppress, decisions[3]);
    assert_eq!(
        press_and_replay(vec![
            buffered(F, KeyTransition::Down),
            buffered(LSHIFT, KeyTransition::Up),
            buffered(F, KeyTransition::Up),
        ]),
        decisions[4]
    );
    assert_eq!(TriggerPhase::HyperActive, state.phase);

    // TriggerHeld (non-typing trigger): Shift no longer activates the layer.
    let caps = config(0x14, TapBehavior::SendTrigger);
    for modifier in [
        0x10, 0x11, 0x12, 0x5B, 0x5C, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5,
    ] {
        let (state, decisions) = run(
            &caps,
            &[down(0x14, 0), down(modifier, 10), up(modifier, 20)],
        );
        assert_eq!(InputDecision::PassThrough, decisions[1], "0x{modifier:02X}");
        assert_eq!(InputDecision::PassThrough, decisions[2], "0x{modifier:02X}");
        assert_eq!(TriggerPhase::TriggerHeld, state.phase, "0x{modifier:02X}");
    }

    // Hyper+Shift+A on a non-typing trigger: Shift passes, A activates.
    let (state, decisions) = run(&caps, &[down(0x14, 0), down(LSHIFT, 10), down(A, 20)]);
    assert_eq!(InputDecision::PressAndForward(MODS.to_vec()), decisions[2]);
    // HyperActive: modifiers are forwarded and the layer stays.
    let shift_up = machine::process_event(state, &up(LSHIFT, 30), &caps);
    assert_eq!(InputDecision::Forward, shift_up.decision);
    assert_eq!(TriggerPhase::HyperActive, shift_up.state.phase);
}

#[test]
fn shift_then_trigger_tap_still_taps() {
    // Shift+Space typed as capitalized text: Shift is held around the tap.
    let (_, decisions) = run(
        &space(),
        &[
            down(LSHIFT, 0),
            down(SPACE, 10),
            up(SPACE, 60),
            up(LSHIFT, 70),
        ],
    );
    assert_eq!(send_trigger_tap(vec![]), decisions[2]);
    assert_eq!(InputDecision::PassThrough, decisions[3]);
}

#[test]
fn buffer_overflow_resolves_as_hold() {
    let mut events = vec![down(SPACE, 0)];
    let codes: Vec<u16> = (0..=MAX_BUFFERED_KEYS as u16).map(|i| A + i).collect();
    for (i, code) in codes.iter().enumerate() {
        events.push(down(*code, 1 + i as u32));
    }
    let (state, decisions) = run(&space(), &events);

    for decision in &decisions[..=MAX_BUFFERED_KEYS] {
        assert_eq!(&InputDecision::Suppress, decision);
    }
    let InputDecision::PressAndReplay { modifiers, keys } = &decisions[MAX_BUFFERED_KEYS + 1]
    else {
        panic!("Expected PressAndReplay, got {:?}", decisions.last());
    };
    assert_eq!(MODS, modifiers.as_slice());
    assert_eq!(MAX_BUFFERED_KEYS + 1, keys.len());
    assert_eq!(codes, keys.iter().map(|key| key.key.0).collect::<Vec<_>>());
    assert_eq!(TriggerPhase::HyperActive, state.phase);
    assert_eq!(None, state.timer_deadline());
}

#[test]
fn trigger_autorepeat_while_held_neither_resolves_nor_buffers() {
    let (state, decisions) = run(
        &space(),
        &[
            down(SPACE, 0),
            down(SPACE, 30),
            down(F, 40),
            down(SPACE, 60),
            down(SPACE, 90),
        ],
    );
    for decision in &decisions {
        assert_eq!(&InputDecision::Suppress, decision);
    }
    assert_eq!(TriggerPhase::TriggerHeld, state.phase);
    assert_eq!(vec![buffered(F, KeyTransition::Down)], state.buffered);
    // The original down time still drives the deadline.
    assert_eq!(Some(HOLD_TIMEOUT_MS), state.timer_deadline());
}

#[test]
fn key_ups_of_keys_pressed_before_the_trigger_pass_through() {
    let (state, decisions) = run(
        &space(),
        &[down(A, 0), down(SPACE, 10), up(A, 20), up(SPACE, 50)],
    );
    assert_eq!(InputDecision::PassThrough, decisions[0]);
    assert_eq!(InputDecision::PassThrough, decisions[2]);
    // A's key-up was not buffered, so the release is a plain tap.
    assert_eq!(send_trigger_tap(vec![]), decisions[3]);
    assert_eq!(TriggerMachineState::IDLE, state);
}

#[test]
fn pre_trigger_key_up_does_not_resolve_a_buffered_hold() {
    let (state, decisions) = run(
        &space(),
        &[down(A, 0), down(SPACE, 10), down(F, 20), up(A, 30)],
    );
    assert_eq!(InputDecision::PassThrough, decisions[3]);
    assert_eq!(TriggerPhase::TriggerHeld, state.phase);
    assert_eq!(vec![buffered(F, KeyTransition::Down)], state.buffered);
}

#[test]
fn long_lone_hold_cancels_the_tap() {
    for trigger in [SPACE, 0x14] {
        let cfg = config(trigger, TapBehavior::SendTrigger);
        let (state, decisions) = run(
            &cfg,
            &[
                down(trigger, 1_000),
                up(trigger, 1_000 + TAP_TIMEOUT_MS + 1),
            ],
        );
        assert_eq!(InputDecision::Suppress, decisions[1], "0x{trigger:02X}");
        assert_eq!(TriggerMachineState::IDLE, state);

        let (_, decisions) = run(
            &cfg,
            &[down(trigger, 1_000), up(trigger, 1_000 + TAP_TIMEOUT_MS)],
        );
        assert_eq!(send_trigger_tap(vec![]), decisions[1], "0x{trigger:02X}");
    }
}

#[test]
fn long_hold_cancel_handles_tick_wraparound() {
    let caps = config(0x14, TapBehavior::SendTrigger);
    let start = u32::MAX - 100;
    let (_, quick) = run(
        &caps,
        &[down(0x14, start), up(0x14, start.wrapping_add(200))],
    );
    assert_eq!(send_trigger_tap(vec![]), quick[1]);
    let (_, long) = run(
        &caps,
        &[down(0x14, start), up(0x14, start.wrapping_add(800))],
    );
    assert_eq!(InputDecision::Suppress, long[1]);
}

#[test]
fn hold_deadline_handles_tick_wraparound() {
    let start = u32::MAX - 50;
    let (state, _) = run(
        &space(),
        &[down(SPACE, start), down(F, start.wrapping_add(10))],
    );
    let deadline = start.wrapping_add(HOLD_TIMEOUT_MS);
    assert_eq!(Some(deadline), state.timer_deadline());
    let early = machine::on_timeout(state.clone(), start.wrapping_add(100), &space());
    assert_eq!(InputDecision::Suppress, early.decision);
    let fired = machine::on_timeout(state, deadline.wrapping_add(1), &space());
    assert!(matches!(
        fired.decision,
        InputDecision::PressAndReplay { .. }
    ));
}

#[test]
fn each_tap_action_is_reported() {
    for behavior in [
        TapBehavior::SendTrigger,
        TapBehavior::Escape,
        TapBehavior::Nothing,
    ] {
        for trigger in [SPACE, 0x14] {
            let cfg = config(trigger, behavior);
            let (_, decisions) = run(&cfg, &[down(trigger, 0), up(trigger, 100)]);
            assert_eq!(
                InputDecision::Tap {
                    action: behavior,
                    then_replay: vec![]
                },
                decisions[1]
            );
        }
        // Rollover carries the configured action too.
        let cfg = config(SPACE, behavior);
        let (_, decisions) = run(&cfg, &[down(SPACE, 0), down(F, 10), up(SPACE, 20)]);
        assert_eq!(
            InputDecision::Tap {
                action: behavior,
                then_replay: vec![buffered(F, KeyTransition::Down)]
            },
            decisions[2]
        );
    }
}

#[test]
fn non_typing_trigger_activates_on_the_first_key_down() {
    let caps = config(0x14, TapBehavior::Escape);
    let (state, decisions) = run(&caps, &[down(0x14, 0), down(F, 10)]);
    assert_eq!(InputDecision::PressAndForward(MODS.to_vec()), decisions[1]);
    assert_eq!(TriggerPhase::HyperActive, state.phase);
}

#[test]
fn buffered_key_autorepeat_is_buffered_then_replayed() {
    let (_, decisions) = run(
        &space(),
        &[down(SPACE, 0), down(F, 10), down(F, 40), up(F, 60)],
    );
    assert_eq!(
        press_and_replay(vec![
            buffered(F, KeyTransition::Down),
            buffered(F, KeyTransition::Down),
            buffered(F, KeyTransition::Up),
        ]),
        decisions[3]
    );
}

#[test]
fn synthetic_events_do_not_buffer() {
    let (state, _) = run(&space(), &[down(SPACE, 0)]);
    let synthetic = KeyboardEvent::synthetic(VirtualKey(F), KeyTransition::Down).at(10);
    let result = machine::process_event(state.clone(), &synthetic, &space());
    assert_eq!(InputDecision::PassThrough, result.decision);
    assert_eq!(state, result.state);
}

#[test]
fn hold_state_never_leaks_into_the_next_cycle() {
    let mut state = TriggerMachineState::IDLE;
    let cfg = space();
    for cycle in 0..5u32 {
        let base = cycle * 1_000;
        for event in [
            down(SPACE, base),
            down(F, base + 10),
            up(SPACE, base + 20),
            up(F, base + 30),
        ] {
            state = machine::process_event(state, &event, &cfg).state;
        }
        assert_eq!(TriggerMachineState::IDLE, state);
    }
}

/// Feeds events the way the engine does: an event stamped at or past the
/// hold deadline first runs the overdue timeout, as if the timer had fired
/// on time. Returns every decision, the catch-up ones included.
fn run_with_catch_up(
    config: &MachineConfig<'_>,
    events: &[KeyboardEvent],
) -> (TriggerMachineState, Vec<InputDecision>) {
    let mut state = TriggerMachineState::IDLE;
    let mut decisions = Vec::new();
    for event in events {
        if state.deadline_passed(event.time_ms) {
            let overdue = machine::on_timeout(state, event.time_ms, config);
            state = overdue.state;
            decisions.push(overdue.decision);
        }
        let next = machine::process_event(state, event, config);
        state = next.state;
        decisions.push(next.decision);
    }
    (state, decisions)
}

#[test]
fn a_late_timer_with_keys_buffered_still_resolves_as_a_hold() {
    // Space down, F down, and Space up stamped past the deadline before
    // WM_TIMER was dispatched: the same outcome as an on-time timer.
    let events = [down(SPACE, 0), down(F, 50), up(SPACE, HOLD_TIMEOUT_MS + 30)];
    let (state, decisions) = run_with_catch_up(&space(), &events);
    assert_eq!(
        vec![
            InputDecision::Suppress,
            InputDecision::Suppress,
            press_and_replay(vec![buffered(F, KeyTransition::Down)]),
            InputDecision::ReleaseAndSuppress(MODS.to_vec()),
        ],
        decisions
    );
    assert_eq!(TriggerMachineState::IDLE, state);

    // The on-time timer gives the identical sequence.
    let (held, _) = run(&space(), &events[..2]);
    let fired = machine::on_timeout(held, HOLD_TIMEOUT_MS, &space());
    let released = machine::process_event(fired.state, &events[2], &space());
    assert_eq!(decisions[2], fired.decision);
    assert_eq!(decisions[3], released.decision);
}

#[test]
fn a_late_timer_with_keys_buffered_forwards_the_next_key_under_the_layer() {
    let (state, decisions) = run_with_catch_up(
        &space(),
        &[down(SPACE, 0), down(F, 50), down(J, HOLD_TIMEOUT_MS)],
    );
    assert_eq!(
        press_and_replay(vec![buffered(F, KeyTransition::Down)]),
        decisions[2]
    );
    assert_eq!(InputDecision::Forward, decisions[3]);
    assert_eq!(TriggerPhase::HyperActive, state.phase);
}

#[test]
fn deadline_passed_tracks_only_an_armed_deadline() {
    let (held, _) = run(&space(), &[down(SPACE, 0)]);
    assert!(!held.deadline_passed(HOLD_TIMEOUT_MS - 1));
    assert!(held.deadline_passed(HOLD_TIMEOUT_MS));
    assert!(!TriggerMachineState::IDLE.deadline_passed(10_000));
    let caps = config(0x14, TapBehavior::SendTrigger);
    let (non_typing, _) = run(&caps, &[down(0x14, 0)]);
    assert!(!non_typing.deadline_passed(10_000));
}

#[test]
fn a_modifier_released_after_a_buffered_key_replays_after_it() {
    // Shift+B rolled over Space: Shift is released before Space, but B was
    // pressed under Shift, so the tap must replay B before the Shift up.
    let (state, decisions) = run(
        &space(),
        &[
            down(LSHIFT, 0),
            down(SPACE, 10),
            down(F, 20),
            up(LSHIFT, 30),
            up(SPACE, 40),
        ],
    );
    assert_eq!(InputDecision::PassThrough, decisions[0]);
    assert_eq!(InputDecision::Suppress, decisions[3]);
    assert_eq!(
        send_trigger_tap(vec![
            buffered(F, KeyTransition::Down),
            buffered(LSHIFT, KeyTransition::Up),
        ]),
        decisions[4]
    );
    assert_eq!(TriggerMachineState::IDLE, state);
}

#[test]
fn a_modifier_pressed_after_a_buffered_key_replays_after_it() {
    // Space, X, then Shift: Space and X were typed unshifted.
    let (_, decisions) = run(
        &space(),
        &[down(SPACE, 0), down(F, 10), down(LSHIFT, 20), up(SPACE, 30)],
    );
    assert_eq!(InputDecision::Suppress, decisions[2]);
    assert_eq!(
        send_trigger_tap(vec![
            buffered(F, KeyTransition::Down),
            buffered(LSHIFT, KeyTransition::Down),
        ]),
        decisions[3]
    );
}

#[test]
fn buffered_modifiers_replay_in_order_on_a_hold() {
    let (state, decisions) = run(&space(), &[down(SPACE, 0), down(F, 10), down(LSHIFT, 20)]);
    assert_eq!(InputDecision::Suppress, decisions[2]);
    let fired = machine::on_timeout(state, HOLD_TIMEOUT_MS, &space());
    assert_eq!(
        press_and_replay(vec![
            buffered(F, KeyTransition::Down),
            buffered(LSHIFT, KeyTransition::Down),
        ]),
        fired.decision
    );
}

#[test]
fn a_buffered_modifier_neither_resolves_nor_counts_toward_overflow() {
    let mut events = vec![down(SPACE, 0)];
    let mut time = 1;
    for i in 0..MAX_BUFFERED_KEYS as u16 {
        events.push(down(A + i, time));
        events.push(down(LSHIFT, time + 1));
        events.push(up(LSHIFT, time + 2));
        time += 3;
    }
    let (state, decisions) = run(&space(), &events);
    assert!(decisions.iter().all(|d| *d == InputDecision::Suppress));
    assert_eq!(TriggerPhase::TriggerHeld, state.phase);
    assert_eq!(3 * MAX_BUFFERED_KEYS, state.buffered.len());
}
