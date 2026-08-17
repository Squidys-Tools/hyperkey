using System.Collections.Immutable;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Hyperkey.Core;

public sealed record SettingsParseResult(
    HyperkeySettings Settings,
    bool UsedDefaults,
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
            SchemaVersion = settings.SchemaVersion,
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

            if (document.SchemaVersion is not HyperkeySettings.CurrentSchemaVersion)
            {
                throw new InvalidDataException($"Unsupported settings schema: {document.SchemaVersion}.");
            }

            if (document.Enabled is null || document.LaunchAtStartup is null)
            {
                throw new InvalidDataException("The settings document is missing a required boolean value.");
            }

            var trigger = ParseTrigger(document.Trigger);
            var modifiers = ParseModifiers(document.OutputModifiers);
            var tapBehavior = ParseTapBehavior(document.TapBehavior);

            return new SettingsParseResult(
                HyperkeySettings.Create(
                    document.Enabled.Value,
                    trigger,
                    modifiers,
                    document.LaunchAtStartup.Value,
                    document.LaunchToTray ?? false,
                    tapBehavior,
                    document.SchemaVersion.Value),
                UsedDefaults: false,
                Error: null);
        }
        catch (Exception exception) when (exception is JsonException or InvalidDataException or ArgumentException or NotSupportedException)
        {
            return new SettingsParseResult(
                HyperkeySettings.Defaults,
                UsedDefaults: true,
                Error: exception.Message);
        }
    }

    private static TriggerKey ParseTrigger(string? value) => value switch
    {
        not null when string.Equals(value, "CapsLock", StringComparison.OrdinalIgnoreCase) => TriggerKey.CapsLock,
        not null when string.Equals(value, "ScrollLock", StringComparison.OrdinalIgnoreCase) => TriggerKey.ScrollLock,
        _ => throw new InvalidDataException($"Unsupported trigger key: {value ?? "missing"}.")
    };

    private static TapBehavior ParseTapBehavior(string? value) => value switch
    {
        not null when string.Equals(value, "CapsLock", StringComparison.OrdinalIgnoreCase) => TapBehavior.CapsLock,
        // Settings created before tap behavior was implemented used this placeholder.
        not null when string.Equals(value, "Undecided", StringComparison.OrdinalIgnoreCase) => TapBehavior.CapsLock,
        _ => throw new InvalidDataException($"Unsupported tap behavior: {value ?? "missing"}.")
    };

    private static ImmutableArray<OutputModifier> ParseModifiers(string[]? values)
    {
        if (values is null || values.Length is < 1 or > 3)
        {
            throw new InvalidDataException("The settings document must contain one to three output modifiers.");
        }

        var parsed = ImmutableArray.CreateBuilder<OutputModifier>(values.Length);
        foreach (var value in values)
        {
            var modifier = value switch
            {
                not null when string.Equals(value, "Control", StringComparison.OrdinalIgnoreCase) => OutputModifier.Control,
                not null when string.Equals(value, "Alt", StringComparison.OrdinalIgnoreCase) => OutputModifier.Alt,
                not null when string.Equals(value, "Shift", StringComparison.OrdinalIgnoreCase) => OutputModifier.Shift,
                _ => throw new InvalidDataException($"Unsupported output modifier: {value ?? "missing"}.")
            };

            if (parsed.Contains(modifier))
            {
                throw new InvalidDataException("The settings document contains duplicate output modifiers.");
            }

            parsed.Add(modifier);
        }

        return parsed.ToImmutable();
    }

    private static string ToWireName(TriggerKey trigger) => trigger switch
    {
        TriggerKey.CapsLock => "CapsLock",
        TriggerKey.ScrollLock => "ScrollLock",
        _ => throw new ArgumentOutOfRangeException(nameof(trigger))
    };

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
