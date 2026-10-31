using System.Collections.Immutable;
using Hyperkey.App.Services;
using Hyperkey.Core;
using Xunit;

namespace Hyperkey.Core.Tests;

public class SettingsMigrationTests
{
    [Fact]
    public void UnversionedDocumentMigratesAndKeepsValues()
    {
        const string unversioned = """
            {
              "enabled": false,
              "trigger": "F9",
              "outputModifiers": ["Control", "Shift"],
              "launchAtStartup": false,
              "launchToTray": true,
              "tapBehavior": "CapsLock"
            }
            """;

        var result = SettingsJson.Parse(unversioned);

        Assert.False(result.UsedDefaults);
        Assert.False(result.Repaired);
        Assert.Equal(HyperkeySettings.CurrentSchemaVersion, result.Settings.SchemaVersion);
        Assert.False(result.Settings.Enabled);
        Assert.True(TriggerKey.TryParse("F9", out var expectedTrigger), "Expected F9 to parse.");
        Assert.Equal(expectedTrigger, result.Settings.Trigger);
        Assert.Equal<OutputModifier>(
            ImmutableArray.Create(OutputModifier.Control, OutputModifier.Shift),
            result.Settings.OutputModifiers);
        Assert.True(result.Settings.LaunchToTray);
    }

    [Fact]
    public void UnversionedDocumentMissingNewerFieldsStillMigrates()
    {
        const string unversioned = """
            {
              "enabled": true,
              "trigger": "ScrollLock",
              "outputModifiers": ["Alt"],
              "launchAtStartup": false
            }
            """;

        var result = SettingsJson.Parse(unversioned);

        Assert.False(result.UsedDefaults);
        Assert.False(result.Repaired);
        Assert.Equal(TriggerKey.ScrollLock, result.Settings.Trigger);
        Assert.Equal<OutputModifier>(ImmutableArray.Create(OutputModifier.Alt), result.Settings.OutputModifiers);
        Assert.False(result.Settings.LaunchToTray);
        Assert.Equal(TapBehavior.CapsLock, result.Settings.TapBehavior);
    }

    [Fact]
    public void InvalidTriggerResetsToDefaultAndPreservesOtherValues()
    {
        const string document = """
            {
              "schemaVersion": 2,
              "enabled": false,
              "trigger": "Win",
              "outputModifiers": ["Alt"],
              "launchAtStartup": false,
              "launchToTray": true,
              "tapBehavior": "CapsLock"
            }
            """;

        var result = SettingsJson.Parse(document);

        Assert.False(result.UsedDefaults);
        Assert.True(result.Repaired);
        Assert.Equal(HyperkeySettings.Defaults.Trigger, result.Settings.Trigger);
        Assert.False(result.Settings.Enabled);
        Assert.True(result.Settings.LaunchToTray);
        Assert.Equal<OutputModifier>(ImmutableArray.Create(OutputModifier.Alt), result.Settings.OutputModifiers);
    }

    [Fact]
    public void InvalidOutputModifiersResetToDefaults()
    {
        var template = """
            {
              "schemaVersion": 2,
              "enabled": true,
              "trigger": "CapsLock",
              "outputModifiers": MODIFIERS,
              "launchAtStartup": true
            }
            """;

        var empty = SettingsJson.Parse(template.Replace("MODIFIERS", "[]", StringComparison.Ordinal));
        Assert.False(empty.UsedDefaults);
        Assert.True(empty.Repaired);
        Assert.Equal<OutputModifier>(HyperkeySettings.Defaults.OutputModifiers, empty.Settings.OutputModifiers);

        var unknownOnly = SettingsJson.Parse(template.Replace("MODIFIERS", "[\"Meta\"]", StringComparison.Ordinal));
        Assert.True(unknownOnly.Repaired);
        Assert.Equal<OutputModifier>(HyperkeySettings.Defaults.OutputModifiers, unknownOnly.Settings.OutputModifiers);

        var mixed = SettingsJson.Parse(template.Replace(
            "MODIFIERS", "[\"Control\", \"Meta\", \"Shift\", \"Shift\"]", StringComparison.Ordinal));
        Assert.False(mixed.UsedDefaults);
        Assert.True(mixed.Repaired);
        Assert.Equal<OutputModifier>(
            ImmutableArray.Create(OutputModifier.Control, OutputModifier.Shift),
            mixed.Settings.OutputModifiers);
    }

    [Fact]
    public void UnknownTapBehaviorResetsToCapsLock()
    {
        var template = """
            {
              "schemaVersion": 2,
              "enabled": true,
              "trigger": "CapsLock",
              "outputModifiers": ["Control"],
              "launchAtStartup": true,
              "tapBehavior": TAPBEHAVIOR
            }
            """;

        var unknown = SettingsJson.Parse(template.Replace("TAPBEHAVIOR", "\"Invert\"", StringComparison.Ordinal));
        Assert.False(unknown.UsedDefaults);
        Assert.True(unknown.Repaired);
        Assert.Equal(TapBehavior.CapsLock, unknown.Settings.TapBehavior);

        var legacy = SettingsJson.Parse(template.Replace("TAPBEHAVIOR", "\"Undecided\"", StringComparison.Ordinal));
        Assert.False(legacy.UsedDefaults);
        Assert.False(legacy.Repaired);
        Assert.Equal(TapBehavior.CapsLock, legacy.Settings.TapBehavior);
    }

    [Fact]
    public void MissingRequiredBooleansResetToDefaults()
    {
        const string document = """
            {
              "schemaVersion": 2,
              "trigger": "CapsLock",
              "outputModifiers": ["Control"]
            }
            """;

        var result = SettingsJson.Parse(document);

        Assert.False(result.UsedDefaults);
        Assert.True(result.Repaired);
        Assert.Equal(HyperkeySettings.Defaults.Enabled, result.Settings.Enabled);
        Assert.Equal(HyperkeySettings.Defaults.LaunchAtStartup, result.Settings.LaunchAtStartup);
        Assert.Equal<OutputModifier>(ImmutableArray.Create(OutputModifier.Control), result.Settings.OutputModifiers);
    }

    [Fact]
    public void UnsupportedSchemaVersionFallsBackToDefaults()
    {
        const string document = """
            {
              "schemaVersion": 99,
              "enabled": false,
              "trigger": "F9",
              "outputModifiers": ["Alt"],
              "launchAtStartup": false,
              "launchToTray": true,
              "tapBehavior": "CapsLock"
            }
            """;

        var result = SettingsJson.Parse(document);

        Assert.True(result.UsedDefaults);
        Assert.False(result.Repaired);
        Assert.Equal(HyperkeySettings.Defaults, result.Settings);
        Assert.True(result.Error is not null, "Expected a schema error to be reported.");
    }

    [Fact]
    public void CorruptJsonFallsBackToDefaults()
    {
        var result = SettingsJson.Parse("{ \"enabled\": true, triger: [");

        Assert.True(result.UsedDefaults);
        Assert.False(result.Repaired);
        Assert.Equal(HyperkeySettings.Defaults, result.Settings);
        Assert.True(result.Error is not null, "Expected a parse error to be reported.");
    }

    [Fact]
    public void CorruptFileIsPreservedAsBackupAndDefaultsLoad()
    {
        var directory = CreateTempDirectory();
        try
        {
            var settingsPath = Path.Combine(directory, "settings.json");
            File.WriteAllText(settingsPath, "{ not valid json !!!");
            var store = new SettingsStore(directory);

            var result = store.Load();

            Assert.True(result.UsedDefaults);
            Assert.Equal(HyperkeySettings.Defaults, result.Settings);
            Assert.True(result.Message is not null, "Expected a recovery message.");
            Assert.False(File.Exists(settingsPath), "Expected the corrupt file to be moved aside.");
            Assert.Equal("{ not valid json !!!", File.ReadAllText($"{settingsPath}.bak"));
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public void SavedSettingsRoundTripAndLeaveNoTempFile()
    {
        var directory = CreateTempDirectory();
        try
        {
            var store = new SettingsStore(directory);
            var configured = HyperkeySettings.Defaults
                .WithEnabled(false)
                .WithOutputModifiers(ImmutableArray.Create(OutputModifier.Alt));

            var saveResult = store.Save(configured);
            Assert.True(saveResult.Succeeded);
            Assert.False(File.Exists(Path.Combine(directory, "settings.json.tmp")), "Expected no leftover temp file.");

            var loadResult = store.Load();
            Assert.False(loadResult.UsedDefaults);
            Assert.True(loadResult.Message is null, "Expected no recovery message.");
            Assert.Equal(configured.Enabled, loadResult.Settings.Enabled);
            Assert.Equal(configured.Trigger, loadResult.Settings.Trigger);
            Assert.Equal<OutputModifier>(configured.OutputModifiers, loadResult.Settings.OutputModifiers);
            Assert.Equal(configured.LaunchAtStartup, loadResult.Settings.LaunchAtStartup);
            Assert.Equal(configured.LaunchToTray, loadResult.Settings.LaunchToTray);
            Assert.Equal(configured.TapBehavior, loadResult.Settings.TapBehavior);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public void UnsupportedSchemaFileStaysOnDiskAndLoadsDefaults()
    {
        var directory = CreateTempDirectory();
        try
        {
            var settingsPath = Path.Combine(directory, "settings.json");
            const string newerDocument = """
                {
                  "schemaVersion": 99,
                  "enabled": false,
                  "trigger": "F9",
                  "outputModifiers": ["Alt"],
                  "launchAtStartup": false,
                  "launchToTray": true,
                  "tapBehavior": "CapsLock"
                }
                """;
            File.WriteAllText(settingsPath, newerDocument);
            var store = new SettingsStore(directory);

            var result = store.Load();

            Assert.True(result.UsedDefaults);
            Assert.Equal(HyperkeySettings.Defaults, result.Settings);
            Assert.True(result.Message is not null, "Expected a recovery message.");
            Assert.Equal(newerDocument, File.ReadAllText(settingsPath));
            Assert.False(File.Exists($"{settingsPath}.bak"), "Expected no backup for a readable file.");
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public void RepairedFileKeepsOriginalAndReportsMessage()
    {
        var directory = CreateTempDirectory();
        try
        {
            var settingsPath = Path.Combine(directory, "settings.json");
            File.WriteAllText(settingsPath, """
                {
                  "schemaVersion": 2,
                  "enabled": true,
                  "trigger": "NotAKey",
                  "outputModifiers": ["Control"],
                  "launchAtStartup": true,
                  "launchToTray": false,
                  "tapBehavior": "CapsLock"
                }
                """);
            var store = new SettingsStore(directory);

            var result = store.Load();

            Assert.False(result.UsedDefaults);
            Assert.Equal(HyperkeySettings.Defaults.Trigger, result.Settings.Trigger);
            Assert.True(result.Message is not null, "Expected a repair message.");
            Assert.True(File.Exists(settingsPath), "Expected a repairable file to stay in place.");
            Assert.False(File.Exists($"{settingsPath}.bak"), "Expected no backup for a repairable file.");
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    private static string CreateTempDirectory()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"hyperkey-tests-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        return directory;
    }
}
