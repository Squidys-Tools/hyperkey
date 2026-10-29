using System.Collections.Immutable;
using Hyperkey.App.Services;
using Hyperkey.Core;

namespace Hyperkey.Core.Tests;

internal static class SettingsMigrationTests
{
    public static void UnversionedDocumentMigratesAndKeepsValues()
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

        AssertEqual(false, result.UsedDefaults);
        AssertEqual(false, result.Repaired);
        AssertEqual(HyperkeySettings.CurrentSchemaVersion, result.Settings.SchemaVersion);
        AssertEqual(false, result.Settings.Enabled);
        AssertTrue(TriggerKey.TryParse("F9", out var expectedTrigger), "Expected F9 to parse.");
        AssertEqual(expectedTrigger, result.Settings.Trigger);
        AssertSequenceEqual(
            ImmutableArray.Create(OutputModifier.Control, OutputModifier.Shift),
            result.Settings.OutputModifiers);
        AssertEqual(true, result.Settings.LaunchToTray);
    }

    public static void UnversionedDocumentMissingNewerFieldsStillMigrates()
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

        AssertEqual(false, result.UsedDefaults);
        AssertEqual(false, result.Repaired);
        AssertEqual(TriggerKey.ScrollLock, result.Settings.Trigger);
        AssertSequenceEqual(ImmutableArray.Create(OutputModifier.Alt), result.Settings.OutputModifiers);
        AssertEqual(false, result.Settings.LaunchToTray);
        AssertEqual(TapBehavior.CapsLock, result.Settings.TapBehavior);
    }

    public static void InvalidTriggerResetsToDefaultAndPreservesOtherValues()
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

        AssertEqual(false, result.UsedDefaults);
        AssertEqual(true, result.Repaired);
        AssertEqual(HyperkeySettings.Defaults.Trigger, result.Settings.Trigger);
        AssertEqual(false, result.Settings.Enabled);
        AssertEqual(true, result.Settings.LaunchToTray);
        AssertSequenceEqual(ImmutableArray.Create(OutputModifier.Alt), result.Settings.OutputModifiers);
    }

    public static void InvalidOutputModifiersResetToDefaults()
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
        AssertEqual(false, empty.UsedDefaults);
        AssertEqual(true, empty.Repaired);
        AssertSequenceEqual(HyperkeySettings.Defaults.OutputModifiers, empty.Settings.OutputModifiers);

        var unknownOnly = SettingsJson.Parse(template.Replace("MODIFIERS", "[\"Meta\"]", StringComparison.Ordinal));
        AssertEqual(true, unknownOnly.Repaired);
        AssertSequenceEqual(HyperkeySettings.Defaults.OutputModifiers, unknownOnly.Settings.OutputModifiers);

        var mixed = SettingsJson.Parse(template.Replace(
            "MODIFIERS", "[\"Control\", \"Meta\", \"Shift\", \"Shift\"]", StringComparison.Ordinal));
        AssertEqual(false, mixed.UsedDefaults);
        AssertEqual(true, mixed.Repaired);
        AssertSequenceEqual(
            ImmutableArray.Create(OutputModifier.Control, OutputModifier.Shift),
            mixed.Settings.OutputModifiers);
    }

    public static void UnknownTapBehaviorResetsToCapsLock()
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
        AssertEqual(false, unknown.UsedDefaults);
        AssertEqual(true, unknown.Repaired);
        AssertEqual(TapBehavior.CapsLock, unknown.Settings.TapBehavior);

        var legacy = SettingsJson.Parse(template.Replace("TAPBEHAVIOR", "\"Undecided\"", StringComparison.Ordinal));
        AssertEqual(false, legacy.UsedDefaults);
        AssertEqual(false, legacy.Repaired);
        AssertEqual(TapBehavior.CapsLock, legacy.Settings.TapBehavior);
    }

    public static void MissingRequiredBooleansResetToDefaults()
    {
        const string document = """
            {
              "schemaVersion": 2,
              "trigger": "CapsLock",
              "outputModifiers": ["Control"]
            }
            """;

        var result = SettingsJson.Parse(document);

        AssertEqual(false, result.UsedDefaults);
        AssertEqual(true, result.Repaired);
        AssertEqual(HyperkeySettings.Defaults.Enabled, result.Settings.Enabled);
        AssertEqual(HyperkeySettings.Defaults.LaunchAtStartup, result.Settings.LaunchAtStartup);
        AssertSequenceEqual(ImmutableArray.Create(OutputModifier.Control), result.Settings.OutputModifiers);
    }

    public static void UnsupportedSchemaVersionFallsBackToDefaults()
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

        AssertEqual(true, result.UsedDefaults);
        AssertEqual(false, result.Repaired);
        AssertEqual(HyperkeySettings.Defaults, result.Settings);
        AssertTrue(result.Error is not null, "Expected a schema error to be reported.");
    }

    public static void CorruptJsonFallsBackToDefaults()
    {
        var result = SettingsJson.Parse("{ \"enabled\": true, triger: [");

        AssertEqual(true, result.UsedDefaults);
        AssertEqual(false, result.Repaired);
        AssertEqual(HyperkeySettings.Defaults, result.Settings);
        AssertTrue(result.Error is not null, "Expected a parse error to be reported.");
    }

    public static void CorruptFileIsPreservedAsBackupAndDefaultsLoad()
    {
        var directory = CreateTempDirectory();
        try
        {
            var settingsPath = Path.Combine(directory, "settings.json");
            File.WriteAllText(settingsPath, "{ not valid json !!!");
            var store = new SettingsStore(directory);

            var result = store.Load();

            AssertEqual(true, result.UsedDefaults);
            AssertEqual(HyperkeySettings.Defaults, result.Settings);
            AssertTrue(result.Message is not null, "Expected a recovery message.");
            AssertTrue(!File.Exists(settingsPath), "Expected the corrupt file to be moved aside.");
            AssertEqual("{ not valid json !!!", File.ReadAllText($"{settingsPath}.bak"));
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    public static void SavedSettingsRoundTripAndLeaveNoTempFile()
    {
        var directory = CreateTempDirectory();
        try
        {
            var store = new SettingsStore(directory);
            var configured = HyperkeySettings.Defaults
                .WithEnabled(false)
                .WithOutputModifiers(ImmutableArray.Create(OutputModifier.Alt));

            var saveResult = store.Save(configured);
            AssertEqual(true, saveResult.Succeeded);
            AssertTrue(!File.Exists(Path.Combine(directory, "settings.json.tmp")), "Expected no leftover temp file.");

            var loadResult = store.Load();
            AssertEqual(false, loadResult.UsedDefaults);
            AssertTrue(loadResult.Message is null, "Expected no recovery message.");
            AssertEqual(configured.Enabled, loadResult.Settings.Enabled);
            AssertEqual(configured.Trigger, loadResult.Settings.Trigger);
            AssertSequenceEqual(configured.OutputModifiers, loadResult.Settings.OutputModifiers);
            AssertEqual(configured.LaunchAtStartup, loadResult.Settings.LaunchAtStartup);
            AssertEqual(configured.LaunchToTray, loadResult.Settings.LaunchToTray);
            AssertEqual(configured.TapBehavior, loadResult.Settings.TapBehavior);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    public static void UnsupportedSchemaFileStaysOnDiskAndLoadsDefaults()
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

            AssertEqual(true, result.UsedDefaults);
            AssertEqual(HyperkeySettings.Defaults, result.Settings);
            AssertTrue(result.Message is not null, "Expected a recovery message.");
            AssertEqual(newerDocument, File.ReadAllText(settingsPath));
            AssertTrue(!File.Exists($"{settingsPath}.bak"), "Expected no backup for a readable file.");
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    public static void RepairedFileKeepsOriginalAndReportsMessage()
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

            AssertEqual(false, result.UsedDefaults);
            AssertEqual(HyperkeySettings.Defaults.Trigger, result.Settings.Trigger);
            AssertTrue(result.Message is not null, "Expected a repair message.");
            AssertTrue(File.Exists(settingsPath), "Expected a repairable file to stay in place.");
            AssertTrue(!File.Exists($"{settingsPath}.bak"), "Expected no backup for a repairable file.");
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
