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
            ArbitraryLetterTriggerRunsFullCycle();
            FunctionKeyTriggerTapReplays();
            TriggerKeyAllowlistAcceptsNormalKeys();
            TriggerKeyAllowlistRejectsReservedKeys();
            TriggerWireNamesRoundTrip();
            LegacySchemaVersionOneStillLoads();
            UnknownTriggerNameFallsBackToDefaults();
            EngineLoopback.GeneralizedTriggerWorksEndToEndOnTheRealHook();

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

    private static void ArbitraryLetterTriggerRunsFullCycle()
    {
        AssertTrue(TriggerKey.TryCreate(0x4A, out var trigger), "Expected J to be a supported trigger.");
        AssertEqual("J", trigger.WireName);

        var held = TriggerStateMachine.Process(TriggerMachineState.Idle, Key(0x4A, KeyTransition.Down), trigger, Modifiers);
        AssertEqual(TriggerPhase.TriggerHeld, held.State.Phase);
        AssertType<InputDecision.Suppress>(held.Decision);

        var tapped = TriggerStateMachine.Process(held.State, Key(0x4A, KeyTransition.Up), trigger, Modifiers);
        AssertEqual(TriggerPhase.Idle, tapped.State.Phase);
        AssertType<InputDecision.ReplayTrigger>(tapped.Decision);

        var reheld = TriggerStateMachine.Process(TriggerMachineState.Idle, Key(0x4A, KeyTransition.Down), trigger, Modifiers);
        var active = TriggerStateMachine.Process(reheld.State, Key(0x4B, KeyTransition.Down), trigger, Modifiers);
        AssertEqual(TriggerPhase.HyperActive, active.State.Phase);
        var press = AssertType<InputDecision.PressAndForward>(active.Decision);
        AssertEqual(Modifiers, press.Modifiers);

        var released = TriggerStateMachine.Process(active.State, Key(0x4A, KeyTransition.Up), trigger, Modifiers);
        AssertEqual(TriggerPhase.Idle, released.State.Phase);
        AssertType<InputDecision.ReleaseAndSuppress>(released.Decision);

        // Other letters are ordinary keys while J is the trigger.
        var unrelated = TriggerStateMachine.Process(TriggerMachineState.Idle, Key(0x41, KeyTransition.Down), trigger, Modifiers);
        AssertEqual(TriggerPhase.Idle, unrelated.State.Phase);
        AssertType<InputDecision.PassThrough>(unrelated.Decision);
    }

    private static void FunctionKeyTriggerTapReplays()
    {
        AssertTrue(TriggerKey.TryCreate(0x78, out var trigger), "Expected F9 to be a supported trigger.");
        AssertEqual("F9", trigger.WireName);
        AssertEqual("F9", trigger.DisplayLabel);

        var held = TriggerStateMachine.Process(TriggerMachineState.Idle, Key(0x78, KeyTransition.Down), trigger, Modifiers);
        AssertType<InputDecision.Suppress>(held.Decision);

        var released = TriggerStateMachine.Process(held.State, Key(0x78, KeyTransition.Up), trigger, Modifiers);
        AssertEqual(TriggerPhase.Idle, released.State.Phase);
        AssertType<InputDecision.ReplayTrigger>(released.Decision);
    }

    private static void TriggerKeyAllowlistAcceptsNormalKeys()
    {
        var accepted = new (ushort Code, string Wire, string Label)[]
        {
            (0x08, "Back", "Backspace"),
            (0x09, "Tab", "Tab"),
            (0x0D, "Enter", "Enter"),
            (0x14, "CapsLock", "Caps Lock"),
            (0x20, "Space", "Space"),
            (0x30, "D0", "0"),
            (0x33, "D3", "3"),
            (0x39, "D9", "9"),
            (0x41, "A", "A"),
            (0x4A, "J", "J"),
            (0x5A, "Z", "Z"),
            (0x60, "NumPad0", "NumPad0"),
            (0x65, "NumPad5", "NumPad5"),
            (0x6A, "NumPadMultiply", "Num *"),
            (0x6E, "NumPadDecimal", "Num ."),
            (0x70, "F1", "F1"),
            (0x78, "F9", "F9"),
            (0x87, "F24", "F24"),
            (0x90, "NumLock", "Num Lock"),
            (0x91, "ScrollLock", "Scroll Lock"),
            (0xBA, "OemSemicolon", ";"),
            (0xBC, "OemComma", ","),
            (0xBF, "OemQuestion", "/"),
            (0xC0, "OemTilde", "`"),
            (0xDB, "OemOpenBrackets", "["),
            (0xDC, "OemBackslash", "\\"),
            (0xDE, "OemQuotes", "'"),
        };

        foreach (var (code, wire, label) in accepted)
        {
            AssertTrue(TriggerKey.TryCreate(code, out var trigger), $"Expected 0x{code:X2} to be a supported trigger.");
            AssertEqual(wire, trigger.WireName);
            AssertEqual(label, trigger.DisplayLabel);
            AssertTrue(TriggerKey.IsSupportedKey(code), $"Expected 0x{code:X2} to report supported.");
            AssertTrue(TriggerKey.TryParse(wire, out var parsed), $"Expected wire name {wire} to parse.");
            AssertEqual(trigger, parsed);
            AssertTrue(TriggerKey.TryParse(wire.ToLowerInvariant(), out _), $"Expected wire name {wire} to parse case-insensitively.");
        }

        AssertEqual("CapsLock", TriggerKey.CapsLock.WireName);
        AssertEqual("Caps Lock", TriggerKey.CapsLock.DisplayLabel);
        AssertEqual("ScrollLock", TriggerKey.ScrollLock.WireName);
        AssertEqual("Scroll Lock", TriggerKey.ScrollLock.DisplayLabel);
        AssertEqual("Caps Lock", TriggerKey.CapsLock.ToString());
    }

    private static void TriggerKeyAllowlistRejectsReservedKeys()
    {
        // Modifiers, system-reserved, extended-scan-code, media, and synthetic codes.
        ushort[] rejected =
        {
            0x00, 0x10, 0x11, 0x12, 0x13, 0x1B, 0x25, 0x26, 0x27, 0x28,
            0x2C, 0x2D, 0x2E, 0x5B, 0x5C, 0x5D, 0x6F, 0xA0, 0xA1, 0xA2,
            0xA3, 0xA4, 0xA5, 0xAD, 0xB3, 0xE7, 0xFF
        };

        foreach (var code in rejected)
        {
            AssertTrue(!TriggerKey.TryCreate(code, out _), $"Expected 0x{code:X2} to be rejected as a trigger.");
            AssertTrue(!TriggerKey.IsSupportedKey(code), $"Expected 0x{code:X2} to report unsupported.");
        }

        AssertTrue(!TriggerKey.TryParse(null, out _), "Expected a missing trigger name to be rejected.");
        AssertTrue(!TriggerKey.TryParse("Win", out _), "Expected an unknown trigger name to be rejected.");
        AssertTrue(!TriggerKey.TryParse("Shift", out _), "Expected a modifier name to be rejected as a trigger.");
    }

    private static void TriggerWireNamesRoundTrip()
    {
        foreach (var wire in new[] { "F9", "D3", "Space", "OemComma", "Enter", "J", "CapsLock", "ScrollLock" })
        {
            AssertTrue(TriggerKey.TryParse(wire, out var trigger), $"Expected {wire} to parse.");
            var configured = HyperkeySettings.Defaults.WithTrigger(trigger);
            var result = SettingsJson.Parse(SettingsJson.Serialize(configured));

            AssertEqual(false, result.UsedDefaults);
            AssertEqual(trigger, result.Settings.Trigger);
        }

        var serialized = SettingsJson.Serialize(HyperkeySettings.Defaults);
        if (!serialized.Contains("\"schemaVersion\": 2", StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Expected new settings documents to declare schema version 2.");
        }
    }

    private static void LegacySchemaVersionOneStillLoads()
    {
        var legacyVersionOne = """
            {
              "schemaVersion": 1,
              "enabled": true,
              "trigger": "ScrollLock",
              "outputModifiers": ["Control"],
              "launchAtStartup": false,
              "launchToTray": true,
              "tapBehavior": "CapsLock"
            }
            """;
        var result = SettingsJson.Parse(legacyVersionOne);

        AssertEqual(false, result.UsedDefaults);
        AssertEqual(TriggerKey.ScrollLock, result.Settings.Trigger);
        AssertEqual(1, result.Settings.SchemaVersion);
        AssertEqual(true, result.Settings.LaunchToTray);
    }

    private static void UnknownTriggerNameFallsBackToDefaults()
    {
        var serialized = SettingsJson.Serialize(HyperkeySettings.Defaults)
            .Replace("\"trigger\": \"CapsLock\"", "\"trigger\": \"Win\"", StringComparison.Ordinal);
        var result = SettingsJson.Parse(serialized);

        AssertEqual(true, result.UsedDefaults);
        AssertEqual(HyperkeySettings.Defaults, result.Settings);
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

    private static void AssertTrue(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
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
