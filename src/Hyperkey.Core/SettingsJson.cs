using System.Collections.Immutable;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Hyperkey.Core;

public sealed record SettingsParseResult(
    HyperkeySettings Settings,
    bool UsedDefaults,
    bool Repaired,
    string? Error);

public static class SettingsJson
{
    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        PropertyNameCaseInsensitive = true,
        WriteIndented = true
    };

    public static string Serialize(HyperkeySettings settings)
    {
        var document = new PersistedSettings
        {
            // Files on disk always declare the newest schema this binary writes.
            // Older schema versions remain readable (see Parse).
            SchemaVersion = HyperkeySettings.CurrentSchemaVersion,
            Enabled = settings.Enabled,
            Trigger = ToWireName(settings.Trigger),
            OutputModifiers = settings.OutputModifiers.Select(ToWireName).ToArray(),
            LaunchAtStartup = settings.LaunchAtStartup,
            LaunchToTray = settings.LaunchToTray,
            TapBehavior = ToWireName(settings.TapBehavior)
        };

        return JsonSerializer.Serialize(document, SerializerOptions);
    }

    public static SettingsParseResult Parse(string json)
    {
        try
        {
            var document = JsonSerializer.Deserialize<PersistedSettings>(json, SerializerOptions)
                ?? throw new InvalidDataException("The settings document was empty.");

            // A missing schemaVersion marks a document written before
            // versioning existed; it is migrated by validating its fields
            // against the current schema. Versions newer than this build
            // still fall back to defaults.
            var schemaVersion = document.SchemaVersion ?? HyperkeySettings.CurrentSchemaVersion;
            if (schemaVersion is < 1 or > HyperkeySettings.CurrentSchemaVersion)
            {
                throw new InvalidDataException($"Unsupported settings schema: {document.SchemaVersion}.");
            }

            var repairs = new List<string>();
            var enabled = document.Enabled
                ?? Repair(repairs, "enabled was reset to its default", HyperkeySettings.Defaults.Enabled);
            var launchAtStartup = document.LaunchAtStartup
                ?? Repair(repairs, "launchAtStartup was reset to its default", HyperkeySettings.Defaults.LaunchAtStartup);
            var trigger = TriggerKey.TryParse(document.Trigger, out var parsedTrigger)
                ? parsedTrigger
                : Repair(
                    repairs,
                    $"trigger \"{document.Trigger ?? "missing"}\" was reset to {HyperkeySettings.Defaults.Trigger.DisplayLabel}",
                    HyperkeySettings.Defaults.Trigger);
            var modifiers = ParseModifiers(document.OutputModifiers, repairs);
            var tapBehavior = ParseTapBehavior(document.TapBehavior, repairs);

            return new SettingsParseResult(
                HyperkeySettings.Create(
                    enabled,
                    trigger,
                    modifiers,
                    launchAtStartup,
                    document.LaunchToTray ?? HyperkeySettings.Defaults.LaunchToTray,
                    tapBehavior,
                    schemaVersion),
                UsedDefaults: false,
                Repaired: repairs.Count > 0,
                Error: repairs.Count > 0 ? string.Join("; ", repairs) : null);
        }
        catch (Exception exception) when (exception is JsonException or InvalidDataException or ArgumentException or NotSupportedException)
        {
            return new SettingsParseResult(
                HyperkeySettings.Defaults,
                UsedDefaults: true,
                Repaired: false,
                Error: exception.Message);
        }
    }

    private static T Repair<T>(ICollection<string> repairs, string description, T fallback)
    {
        repairs.Add(description);
        return fallback;
    }

    private static TapBehavior ParseTapBehavior(string? value, ICollection<string> repairs) => value switch
    {
        // Documents written before the field existed are allowed to omit it.
        null => TapBehavior.CapsLock,
        not null when string.Equals(value, "CapsLock", StringComparison.OrdinalIgnoreCase) => TapBehavior.CapsLock,
        // Settings created before tap behavior was implemented used this placeholder.
        not null when string.Equals(value, "Undecided", StringComparison.OrdinalIgnoreCase) => TapBehavior.CapsLock,
        _ => Repair(repairs, $"tap behavior \"{value}\" was reset to Caps Lock", TapBehavior.CapsLock)
    };

    private static ImmutableArray<OutputModifier> ParseModifiers(string[]? values, ICollection<string> repairs)
    {
        const string resetMessage = "output modifiers were reset to their defaults";
        if (values is null || values.Length == 0)
        {
            return Repair(repairs, resetMessage, HyperkeySettings.Defaults.OutputModifiers);
        }

        var parsed = ImmutableArray.CreateBuilder<OutputModifier>(values.Length);
        var droppedInvalid = false;
        foreach (var value in values)
        {
            var modifier = value switch
            {
                not null when string.Equals(value, "Control", StringComparison.OrdinalIgnoreCase) => OutputModifier.Control,
                not null when string.Equals(value, "Alt", StringComparison.OrdinalIgnoreCase) => OutputModifier.Alt,
                not null when string.Equals(value, "Shift", StringComparison.OrdinalIgnoreCase) => OutputModifier.Shift,
                _ => (OutputModifier?)null
            };

            if (modifier is null || parsed.Contains(modifier.Value))
            {
                droppedInvalid = true;
                continue;
            }

            parsed.Add(modifier.Value);
        }

        if (parsed.Count == 0)
        {
            return Repair(repairs, resetMessage, HyperkeySettings.Defaults.OutputModifiers);
        }

        if (droppedInvalid)
        {
            repairs.Add("unknown or duplicate output modifiers were dropped");
        }

        return parsed.ToImmutable();
    }

    private static string ToWireName(TriggerKey trigger) => trigger.WireName;

    private static string ToWireName(OutputModifier modifier) => modifier switch
    {
        OutputModifier.Control => "Control",
        OutputModifier.Alt => "Alt",
        OutputModifier.Shift => "Shift",
        _ => throw new ArgumentOutOfRangeException(nameof(modifier))
    };

    private static string ToWireName(TapBehavior tapBehavior) => tapBehavior switch
    {
        TapBehavior.CapsLock => "CapsLock",
        _ => throw new ArgumentOutOfRangeException(nameof(tapBehavior))
    };

    private sealed class PersistedSettings
    {
        public PersistedSettings()
        {
        }

        [JsonPropertyName("schemaVersion")]
        public int? SchemaVersion { get; init; }

        [JsonPropertyName("enabled")]
        public bool? Enabled { get; init; }

        [JsonPropertyName("trigger")]
        public string? Trigger { get; init; }

        [JsonPropertyName("outputModifiers")]
        public string[]? OutputModifiers { get; init; }

        [JsonPropertyName("launchAtStartup")]
        public bool? LaunchAtStartup { get; init; }

        [JsonPropertyName("launchToTray")]
        public bool? LaunchToTray { get; init; }

        [JsonPropertyName("tapBehavior")]
        public string? TapBehavior { get; init; }
    }
}
