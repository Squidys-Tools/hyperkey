using System.Collections.Immutable;
using Hyperkey.Core;
using Xunit;

namespace Hyperkey.Core.Tests;

public class TriggerStateMachineTests
{
    private static readonly ImmutableArray<OutputModifier> Modifiers = HyperkeySettings.Defaults.OutputModifiers;

    [Fact]
    public void CapsLockDownIsSuppressedAndEntersTriggerHeld()
    {
        var transition = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));

        Assert.Equal(TriggerPhase.TriggerHeld, transition.State.Phase);
        Assert.IsType<InputDecision.Suppress>(transition.Decision);
    }

    [Fact]
    public void CapsLockTapReplaysNormalCapsLockAndReturnsToIdle()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var released = Process(held.State, CapsLock(KeyTransition.Up));

        Assert.IsType<InputDecision.Suppress>(held.Decision);
        Assert.IsType<InputDecision.ReplayTrigger>(released.Decision);
        Assert.Equal(TriggerPhase.Idle, released.State.Phase);
    }

    [Fact]
    public void FirstKeyDownPressesModifiersAndForwardsTheKey()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var activated = Process(held.State, Key(0x41, KeyTransition.Down));

        var press = Assert.IsType<InputDecision.PressAndForward>(activated.Decision);
        Assert.Equal(TriggerPhase.HyperActive, activated.State.Phase);
        Assert.Equal<OutputModifier>(Modifiers, press.Modifiers);
    }

    [Fact]
    public void ConfiguredScrollLockActsAsTheTrigger()
    {
        var capsLock = Process(
            TriggerMachineState.Idle,
            CapsLock(KeyTransition.Down),
            TriggerKey.ScrollLock);
        var scrollLock = Process(
            TriggerMachineState.Idle,
            new KeyboardEvent(VirtualKey.ScrollLock, KeyTransition.Down),
            TriggerKey.ScrollLock);

        Assert.Equal(TriggerPhase.Idle, capsLock.State.Phase);
        Assert.IsType<InputDecision.PassThrough>(capsLock.Decision);
        Assert.Equal(TriggerPhase.TriggerHeld, scrollLock.State.Phase);
        Assert.IsType<InputDecision.Suppress>(scrollLock.Decision);
    }

    [Fact]
    public void HyperActiveForwardsOtherKeysUntilCapsLockRelease()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var active = Process(held.State, Key(0x41, KeyTransition.Down));
        var forwarded = Process(active.State, Key(0x41, KeyTransition.Up));
        var released = Process(forwarded.State, CapsLock(KeyTransition.Up));

        Assert.IsType<InputDecision.Forward>(forwarded.Decision);
        Assert.Equal(TriggerPhase.HyperActive, forwarded.State.Phase);
        Assert.IsType<InputDecision.ReleaseAndSuppress>(released.Decision);
        Assert.Equal(TriggerPhase.Idle, released.State.Phase);
    }

    [Fact]
    public void UnrelatedKeysPassThroughWhileIdle()
    {
        var transition = Process(TriggerMachineState.Idle, Key(0x41, KeyTransition.Down));

        Assert.Equal(TriggerPhase.Idle, transition.State.Phase);
        Assert.IsType<InputDecision.PassThrough>(transition.Decision);
    }

    [Fact]
    public void TriggerRepeatRemainsSuppressed()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var repeated = Process(held.State, CapsLock(KeyTransition.Down));

        Assert.Equal(TriggerPhase.TriggerHeld, repeated.State.Phase);
        Assert.IsType<InputDecision.Suppress>(repeated.Decision);
    }

    [Fact]
    public void KeyUpBeforeActivationPassesThrough()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var unrelatedKeyUp = Process(held.State, Key(0x41, KeyTransition.Up));

        Assert.Equal(TriggerPhase.TriggerHeld, unrelatedKeyUp.State.Phase);
        Assert.IsType<InputDecision.PassThrough>(unrelatedKeyUp.Decision);
    }

    [Fact]
    public void SyntheticEventsNeverChangeTheState()
    {
        var syntheticCapsLock = new KeyboardEvent(VirtualKey.CapsLock, KeyTransition.Down, IsSynthetic: true);
        var transition = Process(TriggerMachineState.Idle, syntheticCapsLock);

        Assert.Equal(TriggerPhase.Idle, transition.State.Phase);
        Assert.IsType<InputDecision.PassThrough>(transition.Decision);
    }

    [Fact]
    public void SyntheticEventsPassThroughWhileHyperActive()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var active = Process(held.State, Key(0x41, KeyTransition.Down));
        var synthetic = Process(
            active.State,
            new KeyboardEvent(new VirtualKey(0x42), KeyTransition.Down, IsSynthetic: true));

        Assert.Equal(TriggerPhase.HyperActive, synthetic.State.Phase);
        Assert.IsType<InputDecision.PassThrough>(synthetic.Decision);
    }

    [Fact]
    public void RepeatedCyclesDoNotAccumulateState()
    {
        var state = TriggerMachineState.Idle;
        for (var cycle = 0; cycle < 10; cycle++)
        {
            var held = Process(state, CapsLock(KeyTransition.Down));
            var active = Process(held.State, Key(0x41, KeyTransition.Down));
            var released = Process(active.State, CapsLock(KeyTransition.Up));
            state = released.State;

            Assert.Equal(TriggerPhase.Idle, state.Phase);
            Assert.IsType<InputDecision.ReleaseAndSuppress>(released.Decision);
        }
    }

    [Fact]
    public void ArbitraryLetterTriggerRunsFullCycle()
    {
        Assert.True(TriggerKey.TryCreate(0x4A, out var trigger), "Expected J to be a supported trigger.");
        Assert.Equal("J", trigger.WireName);

        var held = TriggerStateMachine.Process(TriggerMachineState.Idle, Key(0x4A, KeyTransition.Down), trigger, Modifiers);
        Assert.Equal(TriggerPhase.TriggerHeld, held.State.Phase);
        Assert.IsType<InputDecision.Suppress>(held.Decision);

        var tapped = TriggerStateMachine.Process(held.State, Key(0x4A, KeyTransition.Up), trigger, Modifiers);
        Assert.Equal(TriggerPhase.Idle, tapped.State.Phase);
        Assert.IsType<InputDecision.ReplayTrigger>(tapped.Decision);

        var reheld = TriggerStateMachine.Process(TriggerMachineState.Idle, Key(0x4A, KeyTransition.Down), trigger, Modifiers);
        var active = TriggerStateMachine.Process(reheld.State, Key(0x4B, KeyTransition.Down), trigger, Modifiers);
        Assert.Equal(TriggerPhase.HyperActive, active.State.Phase);
        var press = Assert.IsType<InputDecision.PressAndForward>(active.Decision);
        Assert.Equal<OutputModifier>(Modifiers, press.Modifiers);

        var released = TriggerStateMachine.Process(active.State, Key(0x4A, KeyTransition.Up), trigger, Modifiers);
        Assert.Equal(TriggerPhase.Idle, released.State.Phase);
        Assert.IsType<InputDecision.ReleaseAndSuppress>(released.Decision);

        // Other letters are ordinary keys while J is the trigger.
        var unrelated = TriggerStateMachine.Process(TriggerMachineState.Idle, Key(0x41, KeyTransition.Down), trigger, Modifiers);
        Assert.Equal(TriggerPhase.Idle, unrelated.State.Phase);
        Assert.IsType<InputDecision.PassThrough>(unrelated.Decision);
    }

    [Fact]
    public void FunctionKeyTriggerTapReplays()
    {
        Assert.True(TriggerKey.TryCreate(0x78, out var trigger), "Expected F9 to be a supported trigger.");
        Assert.Equal("F9", trigger.WireName);
        Assert.Equal("F9", trigger.DisplayLabel);

        var held = TriggerStateMachine.Process(TriggerMachineState.Idle, Key(0x78, KeyTransition.Down), trigger, Modifiers);
        Assert.IsType<InputDecision.Suppress>(held.Decision);

        var released = TriggerStateMachine.Process(held.State, Key(0x78, KeyTransition.Up), trigger, Modifiers);
        Assert.Equal(TriggerPhase.Idle, released.State.Phase);
        Assert.IsType<InputDecision.ReplayTrigger>(released.Decision);
    }

    [Fact]
    public void RapidTriggerTapsReplayEachCycle()
    {
        // Back-to-back taps must each replay the trigger and leave the machine
        // in Idle, so a fast double- or triple-tap behaves like N normal presses.
        var state = TriggerMachineState.Idle;
        for (var tap = 0; tap < 5; tap++)
        {
            var held = Process(state, CapsLock(KeyTransition.Down));
            var released = Process(held.State, CapsLock(KeyTransition.Up));
            state = released.State;

            Assert.IsType<InputDecision.Suppress>(held.Decision);
            Assert.IsType<InputDecision.ReplayTrigger>(released.Decision);
            Assert.Equal(TriggerPhase.Idle, state.Phase);
        }
    }

    [Fact]
    public void TapThenHoldStillActivatesTheChord()
    {
        // Double-tap-hold (tap, then press and keep holding) must engage the
        // layer on the second press exactly like a first hold.
        var tap = Process(Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down)).State, CapsLock(KeyTransition.Up));
        Assert.IsType<InputDecision.ReplayTrigger>(tap.Decision);

        var held = Process(tap.State, CapsLock(KeyTransition.Down));
        var active = Process(held.State, Key(0x41, KeyTransition.Down));

        var press = Assert.IsType<InputDecision.PressAndForward>(active.Decision);
        Assert.Equal(TriggerPhase.HyperActive, active.State.Phase);
        Assert.Equal<OutputModifier>(Modifiers, press.Modifiers);
    }

    [Fact]
    public void SoloTriggerTapNeverTouchesOutputModifiers()
    {
        // Tapping the trigger alone must only replay the trigger: no modifier
        // press or release decision may be produced at any point.
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var released = Process(held.State, CapsLock(KeyTransition.Up));

        Assert.IsType<InputDecision.Suppress>(held.Decision);
        Assert.IsType<InputDecision.ReplayTrigger>(released.Decision);
        Assert.IsNotType<InputDecision.PressAndForward>(held.Decision);
        Assert.IsNotType<InputDecision.ReleaseAndSuppress>(released.Decision);

        // A second tap afterwards behaves identically.
        var secondTap = Process(Process(released.State, CapsLock(KeyTransition.Down)).State, CapsLock(KeyTransition.Up));
        Assert.IsType<InputDecision.ReplayTrigger>(secondTap.Decision);
        Assert.Equal(TriggerPhase.Idle, secondTap.State.Phase);
    }

    [Fact]
    public void TriggerAutoRepeatWhileHeldStaysSuppressed()
    {
        // Holding the trigger produces repeated key-down events; each must stay
        // suppressed and must not be mistaken for a new press.
        var state = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down)).State;
        for (var repeat = 0; repeat < 8; repeat++)
        {
            var repeated = Process(state, CapsLock(KeyTransition.Down));
            state = repeated.State;

            Assert.Equal(TriggerPhase.TriggerHeld, state.Phase);
            Assert.IsType<InputDecision.Suppress>(repeated.Decision);
        }
    }

    [Fact]
    public void TriggerAutoRepeatWhileHyperActiveStaysSuppressed()
    {
        var active = Process(
            Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down)).State,
            Key(0x41, KeyTransition.Down));
        Assert.Equal(TriggerPhase.HyperActive, active.State.Phase);

        var repeated = Process(active.State, CapsLock(KeyTransition.Down));
        Assert.Equal(TriggerPhase.HyperActive, repeated.State.Phase);
        Assert.IsType<InputDecision.Suppress>(repeated.Decision);
    }

    [Fact]
    public void ChordKeyAutoRepeatForwardsWithoutRepressingModifiers()
    {
        // Auto-repeat of the chord key arrives as extra key-downs while the
        // layer is already active; each is forwarded and modifiers are pressed
        // only once (the first activation).
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var state = held.State;

        var first = Process(state, Key(0x41, KeyTransition.Down));
        Assert.IsType<InputDecision.PressAndForward>(first.Decision);
        state = first.State;

        var pressCount = 0;
        for (var repeat = 0; repeat < 6; repeat++)
        {
            var repeated = Process(state, Key(0x41, KeyTransition.Down));
            state = repeated.State;

            Assert.Equal(TriggerPhase.HyperActive, state.Phase);
            Assert.IsType<InputDecision.Forward>(repeated.Decision);
            if (repeated.Decision is InputDecision.PressAndForward)
            {
                pressCount++;
            }
        }

        Assert.Equal(0, pressCount);
    }

    [Fact]
    public void KeyHeldBeforeTriggerActivatesLayerOnAutoRepeat()
    {
        // A key already held when the trigger is pressed generates auto-repeat
        // key-downs in TriggerHeld. The machine cannot tell these from a fresh
        // chord press, so the first repeat activates the layer. This documents
        // current behavior (the key then gets the modifier layer applied).
        var keyHeldFirst = Process(TriggerMachineState.Idle, Key(0x41, KeyTransition.Down));
        Assert.IsType<InputDecision.PassThrough>(keyHeldFirst.Decision);

        var held = Process(keyHeldFirst.State, CapsLock(KeyTransition.Down));
        var repeat = Process(held.State, Key(0x41, KeyTransition.Down));

        var press = Assert.IsType<InputDecision.PressAndForward>(repeat.Decision);
        Assert.Equal(TriggerPhase.HyperActive, repeat.State.Phase);
        Assert.Equal<OutputModifier>(Modifiers, press.Modifiers);
    }

    [Fact]
    public void TriggerReleasedMidChordReleasesModifiersAndReturnsToIdle()
    {
        // Release the trigger while the chord key is still held: the layer
        // releases immediately (modifiers must not stick), and later events
        // behave as if the layer was never on.
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var active = Process(held.State, Key(0x41, KeyTransition.Down));
        var released = Process(active.State, CapsLock(KeyTransition.Up));

        var release = Assert.IsType<InputDecision.ReleaseAndSuppress>(released.Decision);
        Assert.Equal<OutputModifier>(Modifiers, release.Modifiers);
        Assert.Equal(TriggerPhase.Idle, released.State.Phase);

        // The still-held chord key's key-up, and any later keys, pass through
        // untouched once the layer has disengaged.
        var chordKeyUp = Process(released.State, Key(0x41, KeyTransition.Up));
        Assert.IsType<InputDecision.PassThrough>(chordKeyUp.Decision);
        Assert.Equal(TriggerPhase.Idle, chordKeyUp.State.Phase);

        var nextKey = Process(chordKeyUp.State, Key(0x42, KeyTransition.Down));
        Assert.IsType<InputDecision.PassThrough>(nextKey.Decision);
        Assert.Equal(TriggerPhase.Idle, nextKey.State.Phase);
    }

    [Fact]
    public void TriggerReleaseBetweenTwoChordKeysStillReleases()
    {
        // Two keys pressed during one hold; releasing the trigger after only
        // the first key's key-up still ends the layer cleanly.
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var first = Process(held.State, Key(0x41, KeyTransition.Down));
        var second = Process(first.State, Key(0x42, KeyTransition.Down));
        var released = Process(second.State, CapsLock(KeyTransition.Up));

        Assert.IsType<InputDecision.PressAndForward>(first.Decision);
        Assert.IsType<InputDecision.Forward>(second.Decision);
        Assert.IsType<InputDecision.ReleaseAndSuppress>(released.Decision);
        Assert.Equal(TriggerPhase.Idle, released.State.Phase);
    }

    [Fact]
    public void StrayTriggerKeyUpPassesThroughWithoutAReplay()
    {
        // A trigger key-up with no preceding key-down (e.g. the hook attached
        // while the key was already held) must not replay a spurious tap.
        var transition = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Up));

        Assert.Equal(TriggerPhase.Idle, transition.State.Phase);
        Assert.IsType<InputDecision.PassThrough>(transition.Decision);
    }

    private static TriggerTransition Process(TriggerMachineState state, KeyboardEvent input) =>
        TriggerStateMachine.Process(state, input, Modifiers);

    private static TriggerTransition Process(
        TriggerMachineState state,
        KeyboardEvent input,
        TriggerKey triggerKey) =>
        TriggerStateMachine.Process(state, input, triggerKey, Modifiers);

    private static KeyboardEvent CapsLock(KeyTransition transition) =>
        new(VirtualKey.CapsLock, transition);

    private static KeyboardEvent Key(ushort key, KeyTransition transition) =>
        new(new VirtualKey(key), transition);
}
