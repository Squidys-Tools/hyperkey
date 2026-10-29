using System.Collections.Immutable;
using Hyperkey.Core;
using Xunit;

namespace Hyperkey.Core.Tests;

public class SettingsTests
{
    [Fact]
    public void CapsLockTapBehaviorRoundTripsAndReadsLegacySettings()
    {
        var serialized = SettingsJson.Serialize(HyperkeySettings.Defaults);
        Assert.Contains("\"tapBehavior\": \"CapsLock\"", serialized, StringComparison.Ordinal);

        var roundTrip = SettingsJson.Parse(serialized);
        Assert.False(roundTrip.UsedDefaults);
        Assert.Equal(TapBehavior.CapsLock, roundTrip.Settings.TapBehavior);
        Assert.False(roundTrip.Settings.LaunchToTray);

        var traySettings = HyperkeySettings.Defaults.WithLaunchToTray(true);
        var trayRoundTrip = SettingsJson.Parse(SettingsJson.Serialize(traySettings));
        Assert.False(trayRoundTrip.UsedDefaults);
        Assert.True(trayRoundTrip.Settings.LaunchToTray);

        var legacy = serialized.Replace(
            "\"tapBehavior\": \"CapsLock\"",
            "\"tapBehavior\": \"Undecided\"",
            StringComparison.Ordinal);
        var legacyResult = SettingsJson.Parse(legacy);
        Assert.False(legacyResult.UsedDefaults);
        Assert.Equal(TapBehavior.CapsLock, legacyResult.Settings.TapBehavior);

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
        Assert.False(missingLaunchToTrayResult.UsedDefaults);
        Assert.False(missingLaunchToTrayResult.Settings.LaunchToTray);
    }

    [Fact]
    public void TriggerAndOutputModifiersRoundTrip()
    {
        var configured = HyperkeySettings.Defaults
            .WithTrigger(TriggerKey.ScrollLock)
            .WithOutputModifiers(ImmutableArray.Create(OutputModifier.Control, OutputModifier.Shift));
        var result = SettingsJson.Parse(SettingsJson.Serialize(configured));

        Assert.False(result.UsedDefaults);
        Assert.Equal(TriggerKey.ScrollLock, result.Settings.Trigger);
        Assert.Equal<OutputModifier>(
            ImmutableArray.Create(OutputModifier.Control, OutputModifier.Shift),
            result.Settings.OutputModifiers);
    }

    [Fact]
    public void TriggerKeyAllowlistAcceptsNormalKeys()
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
            Assert.True(TriggerKey.TryCreate(code, out var trigger), $"Expected 0x{code:X2} to be a supported trigger.");
            Assert.Equal(wire, trigger.WireName);
            Assert.Equal(label, trigger.DisplayLabel);
            Assert.True(TriggerKey.IsSupportedKey(code), $"Expected 0x{code:X2} to report supported.");
            Assert.True(TriggerKey.TryParse(wire, out var parsed), $"Expected wire name {wire} to parse.");
            Assert.Equal(trigger, parsed);
            Assert.True(TriggerKey.TryParse(wire.ToLowerInvariant(), out _), $"Expected wire name {wire} to parse case-insensitively.");
        }

        Assert.Equal("CapsLock", TriggerKey.CapsLock.WireName);
        Assert.Equal("Caps Lock", TriggerKey.CapsLock.DisplayLabel);
        Assert.Equal("ScrollLock", TriggerKey.ScrollLock.WireName);
        Assert.Equal("Scroll Lock", TriggerKey.ScrollLock.DisplayLabel);
        Assert.Equal("Caps Lock", TriggerKey.CapsLock.ToString());
    }

    [Fact]
    public void TriggerKeyAllowlistRejectsReservedKeys()
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
            Assert.False(TriggerKey.TryCreate(code, out _), $"Expected 0x{code:X2} to be rejected as a trigger.");
            Assert.False(TriggerKey.IsSupportedKey(code), $"Expected 0x{code:X2} to report unsupported.");
        }

        Assert.False(TriggerKey.TryParse(null, out _), "Expected a missing trigger name to be rejected.");
        Assert.False(TriggerKey.TryParse("Win", out _), "Expected an unknown trigger name to be rejected.");
        Assert.False(TriggerKey.TryParse("Shift", out _), "Expected a modifier name to be rejected as a trigger.");
    }

    [Fact]
    public void TriggerWireNamesRoundTrip()
    {
        foreach (var wire in new[] { "F9", "D3", "Space", "OemComma", "Enter", "J", "CapsLock", "ScrollLock" })
        {
            Assert.True(TriggerKey.TryParse(wire, out var trigger), $"Expected {wire} to parse.");
            var configured = HyperkeySettings.Defaults.WithTrigger(trigger);
            var result = SettingsJson.Parse(SettingsJson.Serialize(configured));

            Assert.False(result.UsedDefaults);
            Assert.Equal(trigger, result.Settings.Trigger);
        }

        var serialized = SettingsJson.Serialize(HyperkeySettings.Defaults);
        Assert.Contains("\"schemaVersion\": 2", serialized, StringComparison.Ordinal);
    }

    [Fact]
    public void LegacySchemaVersionOneStillLoads()
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

        Assert.False(result.UsedDefaults);
        Assert.Equal(TriggerKey.ScrollLock, result.Settings.Trigger);
        Assert.Equal(1, result.Settings.SchemaVersion);
        Assert.True(result.Settings.LaunchToTray);
    }

    [Fact]
    public void UnknownTriggerNameFallsBackToDefaults()
    {
        var serialized = SettingsJson.Serialize(HyperkeySettings.Defaults)
            .Replace("\"trigger\": \"CapsLock\"", "\"trigger\": \"Win\"", StringComparison.Ordinal);
        var result = SettingsJson.Parse(serialized);

        Assert.True(result.UsedDefaults);
        Assert.Equal(HyperkeySettings.Defaults, result.Settings);
        Assert.NotNull(result.Error);
    }

    [Fact]
    public void MalformedJsonFallsBackToDefaults()
    {
        foreach (var json in new[] { "", "not json", "{", "null", "[1,2,3]", "\"text\"" })
        {
            var result = SettingsJson.Parse(json);

            Assert.True(result.UsedDefaults, $"Expected defaults for malformed input: {json}");
            Assert.Equal(HyperkeySettings.Defaults, result.Settings);
            Assert.NotNull(result.Error);
        }
    }

    [Fact]
    public void UnsupportedSchemaVersionsFallBackToDefaults()
    {
        var serialized = SettingsJson.Serialize(HyperkeySettings.Defaults);
        foreach (var version in new[] { 0, 3, 99 })
        {
            var result = SettingsJson.Parse(serialized.Replace(
                "\"schemaVersion\": 2",
                $"\"schemaVersion\": {version}",
                StringComparison.Ordinal));

            Assert.True(result.UsedDefaults, $"Expected defaults for schemaVersion {version}.");
            Assert.Equal(HyperkeySettings.Defaults, result.Settings);
        }
    }

    [Fact]
    public void MissingRequiredFieldsFallBackToDefaults()
    {
        var withoutEnabled = SettingsJson.Parse("""
            {
              "schemaVersion": 2,
              "trigger": "CapsLock",
              "outputModifiers": ["Control"],
              "launchAtStartup": true,
              "tapBehavior": "CapsLock"
            }
            """);
        Assert.True(withoutEnabled.UsedDefaults);

        var withoutLaunchAtStartup = SettingsJson.Parse("""
            {
              "schemaVersion": 2,
              "enabled": true,
              "trigger": "CapsLock",
              "outputModifiers": ["Control"],
              "tapBehavior": "CapsLock"
            }
            """);
        Assert.True(withoutLaunchAtStartup.UsedDefaults);
    }

    [Fact]
    public void InvalidModifierSetsFallBackToDefaults()
    {
        var modifierSets = new[]
        {
            "[]",
            "[\"Control\", \"Alt\", \"Shift\", \"Control\"]",
            "[\"Control\", \"Control\"]",
            "[\"Win\"]",
            "null",
        };

        foreach (var modifiers in modifierSets)
        {
            var result = SettingsJson.Parse($$"""
                {
                  "schemaVersion": 2,
                  "enabled": true,
                  "trigger": "CapsLock",
                  "outputModifiers": {{modifiers}},
                  "launchAtStartup": true,
                  "tapBehavior": "CapsLock"
                }
                """);

            Assert.True(result.UsedDefaults, $"Expected defaults for outputModifiers {modifiers}.");
            Assert.Equal(HyperkeySettings.Defaults, result.Settings);
        }
    }

    [Fact]
    public void ModifierAndTriggerNamesParseCaseInsensitively()
    {
        var result = SettingsJson.Parse("""
            {
              "schemaVersion": 2,
              "enabled": true,
              "trigger": "capslock",
              "outputModifiers": ["control", "ALT", "sHiFt"],
              "launchAtStartup": false,
              "launchToTray": false,
              "tapBehavior": "capslock"
            }
            """);

        Assert.False(result.UsedDefaults);
        Assert.Equal(TriggerKey.CapsLock, result.Settings.Trigger);
        Assert.Equal<OutputModifier>(
            ImmutableArray.Create(OutputModifier.Control, OutputModifier.Alt, OutputModifier.Shift),
            result.Settings.OutputModifiers);
        Assert.Equal(TapBehavior.CapsLock, result.Settings.TapBehavior);
    }

    [Fact]
    public void MissingLaunchToTrayDefaultsToFalseOnCurrentSchema()
    {
        var result = SettingsJson.Parse("""
            {
              "schemaVersion": 2,
              "enabled": true,
              "trigger": "CapsLock",
              "outputModifiers": ["Control"],
              "launchAtStartup": true,
              "tapBehavior": "CapsLock"
            }
            """);

        Assert.False(result.UsedDefaults);
        Assert.False(result.Settings.LaunchToTray);
    }

    [Fact]
    public void CreateRejectsInvalidModifierSets()
    {
        Assert.Throws<ArgumentException>(() => HyperkeySettings.Defaults.WithOutputModifiers(
            ImmutableArray<OutputModifier>.Empty));

        Assert.Throws<ArgumentException>(() => HyperkeySettings.Defaults.WithOutputModifiers(
            ImmutableArray.Create(
                OutputModifier.Control,
                OutputModifier.Alt,
                OutputModifier.Shift,
                OutputModifier.Control)));

        Assert.Throws<ArgumentException>(() => HyperkeySettings.Defaults.WithOutputModifiers(
            ImmutableArray.Create(OutputModifier.Alt, OutputModifier.Alt)));

        Assert.Throws<ArgumentOutOfRangeException>(() => HyperkeySettings.Defaults.WithOutputModifiers(
            ImmutableArray.Create((OutputModifier)999)));
    }

    [Fact]
    public void CreateRejectsUnsupportedTriggerAndSchema()
    {
        Assert.True(TriggerKey.TryCreate(0x41, out var supported));
        Assert.False(TriggerKey.TryCreate(0x5B, out var winKey));

        Assert.Throws<ArgumentOutOfRangeException>(() => HyperkeySettings.Create(
            enabled: true,
            trigger: winKey,
            outputModifiers: HyperkeySettings.Defaults.OutputModifiers,
            launchAtStartup: true,
            launchToTray: false,
            tapBehavior: TapBehavior.CapsLock));

        Assert.Throws<ArgumentOutOfRangeException>(() => HyperkeySettings.Create(
            enabled: true,
            trigger: supported,
            outputModifiers: HyperkeySettings.Defaults.OutputModifiers,
            launchAtStartup: true,
            launchToTray: false,
            tapBehavior: TapBehavior.CapsLock,
            schemaVersion: 0));

        Assert.Throws<ArgumentOutOfRangeException>(() => HyperkeySettings.Create(
            enabled: true,
            trigger: supported,
            outputModifiers: HyperkeySettings.Defaults.OutputModifiers,
            launchAtStartup: true,
            launchToTray: false,
            tapBehavior: (TapBehavior)99));
    }
}
