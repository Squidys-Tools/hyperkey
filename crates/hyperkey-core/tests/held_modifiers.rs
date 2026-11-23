//! Physical modifier tracking for the chord readout.

use hyperkey_core::keys::{ChordModifier, HeldModifiers, KeyTransition, VirtualKey};
use hyperkey_core::settings::OutputModifier;

#[test]
fn held_modifiers_join_the_generated_ones_in_display_order() {
    let mut held = HeldModifiers::default();
    held.apply(VirtualKey(0xA0), KeyTransition::Down); // Left Shift
    held.apply(VirtualKey(0x5B), KeyTransition::Down); // Left Win
    held.apply(VirtualKey(0x46), KeyTransition::Down); // F: ignored

    assert_eq!(
        vec![
            ChordModifier::Win,
            ChordModifier::Control,
            ChordModifier::Alt,
            ChordModifier::Shift,
        ],
        held.chord_with(&[OutputModifier::Alt, OutputModifier::Control])
    );
}

#[test]
fn a_modifier_both_generated_and_held_appears_once() {
    let mut held = HeldModifiers::default();
    held.apply(VirtualKey(0xA2), KeyTransition::Down); // Left Ctrl
    assert_eq!(
        vec![ChordModifier::Control],
        held.chord_with(&[OutputModifier::Control])
    );
}

#[test]
fn releasing_one_side_keeps_the_other() {
    let mut held = HeldModifiers::default();
    held.apply(VirtualKey(0xA0), KeyTransition::Down);
    held.apply(VirtualKey(0xA1), KeyTransition::Down);
    held.apply(VirtualKey(0xA0), KeyTransition::Down); // autorepeat
    held.apply(VirtualKey(0xA0), KeyTransition::Up);
    assert_eq!(vec![ChordModifier::Shift], held.chord_with(&[]));

    held.apply(VirtualKey(0xA1), KeyTransition::Up);
    assert!(held.chord_with(&[]).is_empty());
}

#[test]
fn clear_forgets_everything_held() {
    let mut held = HeldModifiers::default();
    held.apply(VirtualKey(0xA4), KeyTransition::Down);
    held.clear();
    assert_eq!(
        vec![ChordModifier::Shift],
        held.chord_with(&[OutputModifier::Shift])
    );
}
