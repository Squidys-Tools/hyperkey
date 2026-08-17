using System.Collections.Immutable;
using Hyperkey.Core;

namespace Hyperkey.Core.Tests;

internal static class Program
{
    private static readonly ImmutableArray<OutputModifier> Modifiers = HyperkeySettings.Defaults.OutputModifiers;

    private static int Main()
    {
        try
        {
            CapsLockDownIsSuppressedAndEntersTriggerHeld();
            CapsLockTapReplaysNormalCapsLockAndReturnsToIdle();
            CapsLockTapBehaviorRoundTripsAndReadsLegacySettings();
            TriggerAndOutputModifiersRoundTrip();
            FirstKeyDownPressesModifiersAndForwardsTheKey();
            ConfiguredScrollLockActsAsTheTrigger();
            HyperActiveForwardsOtherKeysUntilCapsLockRelease();
            UnrelatedKeysPassThroughWhileIdle();
            TriggerRepeatRemainsSuppressed();
            KeyUpBeforeActivationPassesThrough();
            SyntheticEventsNeverChangeTheState();
            SyntheticEventsPassThroughWhileHyperActive();
            RepeatedCyclesDoNotAccumulateState();

            Console.WriteLine("Hyperkey.Core.Tests passed.");
            return 0;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(exception.Message);
            return 1;
        }
    }

    private static void CapsLockDownIsSuppressedAndEntersTriggerHeld()
    {
        var transition = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));

        AssertEqual(TriggerPhase.TriggerHeld, transition.State.Phase);
        AssertType<InputDecision.Suppress>(transition.Decision);
    }

    private static void CapsLockTapReplaysNormalCapsLockAndReturnsToIdle()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var released = Process(held.State, CapsLock(KeyTransition.Up));

        AssertType<InputDecision.Suppress>(held.Decision);
        AssertType<InputDecision.ReplayTrigger>(released.Decision);
        AssertEqual(TriggerPhase.Idle, released.State.Phase);
    }

    private static void FirstKeyDownPressesModifiersAndForwardsTheKey()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var activated = Process(held.State, Key(0x41, KeyTransition.Down));

        var press = AssertType<InputDecision.PressAndForward>(activated.Decision);
        AssertEqual(TriggerPhase.HyperActive, activated.State.Phase);
        AssertEqual(Modifiers, press.Modifiers);
    }

    private static void ConfiguredScrollLockActsAsTheTrigger()
    {
        var capsLock = Process(
            TriggerMachineState.Idle,
            CapsLock(KeyTransition.Down),
            TriggerKey.ScrollLock);
        var scrollLock = Process(
            TriggerMachineState.Idle,
            new KeyboardEvent(VirtualKey.ScrollLock, KeyTransition.Down),
            TriggerKey.ScrollLock);

        AssertEqual(TriggerPhase.Idle, capsLock.State.Phase);
        AssertType<InputDecision.PassThrough>(capsLock.Decision);
        AssertEqual(TriggerPhase.TriggerHeld, scrollLock.State.Phase);
        AssertType<InputDecision.Suppress>(scrollLock.Decision);
    }

    private static void CapsLockTapBehaviorRoundTripsAndReadsLegacySettings()
    {
        var serialized = SettingsJson.Serialize(HyperkeySettings.Defaults);
        if (!serialized.Contains("\"tapBehavior\": \"CapsLock\"", StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Expected the current tap behavior to serialize as CapsLock.");
        }

        var roundTrip = SettingsJson.Parse(serialized);
        AssertEqual(false, roundTrip.UsedDefaults);
        AssertEqual(TapBehavior.CapsLock, roundTrip.Settings.TapBehavior);
        AssertEqual(false, roundTrip.Settings.LaunchToTray);

        var traySettings = HyperkeySettings.Defaults.WithLaunchToTray(true);
        var trayRoundTrip = SettingsJson.Parse(SettingsJson.Serialize(traySettings));
        AssertEqual(false, trayRoundTrip.UsedDefaults);
        AssertEqual(true, trayRoundTrip.Settings.LaunchToTray);

        var legacy = serialized.Replace(
            "\"tapBehavior\": \"CapsLock\"",
            "\"tapBehavior\": \"Undecided\"",
            StringComparison.Ordinal);
        var legacyResult = SettingsJson.Parse(legacy);
        AssertEqual(false, legacyResult.UsedDefaults);
        AssertEqual(TapBehavior.CapsLock, legacyResult.Settings.TapBehavior);

        var legacyWithoutLaunchToTray = """
            {
              "schemaVersion": 1,
              "enabled": true,
              "trigger": "CapsLock",
              "outputModifiers": ["Control", "Alt", "Shift"],
              "launchAtStartup": true,
              "tapBehavior": "Undecided"
            }
            """;
        var missingLaunchToTrayResult = SettingsJson.Parse(legacyWithoutLaunchToTray);
        AssertEqual(false, missingLaunchToTrayResult.UsedDefaults);
        AssertEqual(false, missingLaunchToTrayResult.Settings.LaunchToTray);
    }

    private static void TriggerAndOutputModifiersRoundTrip()
    {
        var configured = HyperkeySettings.Defaults
            .WithTrigger(TriggerKey.ScrollLock)
            .WithOutputModifiers(ImmutableArray.Create(OutputModifier.Control, OutputModifier.Shift));
        var result = SettingsJson.Parse(SettingsJson.Serialize(configured));

        AssertEqual(false, result.UsedDefaults);
        AssertEqual(TriggerKey.ScrollLock, result.Settings.Trigger);
        AssertSequenceEqual(
            ImmutableArray.Create(OutputModifier.Control, OutputModifier.Shift),
            result.Settings.OutputModifiers);
    }

    private static void HyperActiveForwardsOtherKeysUntilCapsLockRelease()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var active = Process(held.State, Key(0x41, KeyTransition.Down));
        var forwarded = Process(active.State, Key(0x41, KeyTransition.Up));
        var released = Process(forwarded.State, CapsLock(KeyTransition.Up));

        AssertType<InputDecision.Forward>(forwarded.Decision);
        AssertEqual(TriggerPhase.HyperActive, forwarded.State.Phase);
        AssertType<InputDecision.ReleaseAndSuppress>(released.Decision);
        AssertEqual(TriggerPhase.Idle, released.State.Phase);
    }

    private static void UnrelatedKeysPassThroughWhileIdle()
    {
        var transition = Process(TriggerMachineState.Idle, Key(0x41, KeyTransition.Down));

        AssertEqual(TriggerPhase.Idle, transition.State.Phase);
        AssertType<InputDecision.PassThrough>(transition.Decision);
    }

    private static void TriggerRepeatRemainsSuppressed()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var repeated = Process(held.State, CapsLock(KeyTransition.Down));

        AssertEqual(TriggerPhase.TriggerHeld, repeated.State.Phase);
        AssertType<InputDecision.Suppress>(repeated.Decision);
    }

    private static void KeyUpBeforeActivationPassesThrough()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var unrelatedKeyUp = Process(held.State, Key(0x41, KeyTransition.Up));

        AssertEqual(TriggerPhase.TriggerHeld, unrelatedKeyUp.State.Phase);
        AssertType<InputDecision.PassThrough>(unrelatedKeyUp.Decision);
    }

    private static void SyntheticEventsNeverChangeTheState()
    {
        var syntheticCapsLock = new KeyboardEvent(VirtualKey.CapsLock, KeyTransition.Down, IsSynthetic: true);
        var transition = Process(TriggerMachineState.Idle, syntheticCapsLock);

        AssertEqual(TriggerPhase.Idle, transition.State.Phase);
        AssertType<InputDecision.PassThrough>(transition.Decision);
    }

    private static void SyntheticEventsPassThroughWhileHyperActive()
    {
        var held = Process(TriggerMachineState.Idle, CapsLock(KeyTransition.Down));
        var active = Process(held.State, Key(0x41, KeyTransition.Down));
        var synthetic = Process(
            active.State,
            new KeyboardEvent(new VirtualKey(0x42), KeyTransition.Down, IsSynthetic: true));

        AssertEqual(TriggerPhase.HyperActive, synthetic.State.Phase);
        AssertType<InputDecision.PassThrough>(synthetic.Decision);
    }

    private static void RepeatedCyclesDoNotAccumulateState()
    {
        var state = TriggerMachineState.Idle;
        for (var cycle = 0; cycle < 10; cycle++)
        {
            var held = Process(state, CapsLock(KeyTransition.Down));
            var active = Process(held.State, Key(0x41, KeyTransition.Down));
            var released = Process(active.State, CapsLock(KeyTransition.Up));
            state = released.State;

            AssertEqual(TriggerPhase.Idle, state.Phase);
            AssertType<InputDecision.ReleaseAndSuppress>(released.Decision);
        }
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

    private static T AssertType<T>(InputDecision decision)
        where T : InputDecision
    {
        if (decision is not T typedDecision)
        {
            throw new InvalidOperationException($"Expected {typeof(T).Name}, got {decision.GetType().Name}.");
        }

        return typedDecision;
    }

    private static void AssertEqual<T>(T expected, T actual)
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
        {
            throw new InvalidOperationException($"Expected {expected}, got {actual}.");
        }
    }

    private static void AssertSequenceEqual<T>(IEnumerable<T> expected, IEnumerable<T> actual)
    {
        if (!expected.SequenceEqual(actual))
        {
            throw new InvalidOperationException(
                $"Expected [{string.Join(", ", expected)}], got [{string.Join(", ", actual)}].");
        }
    }
}
