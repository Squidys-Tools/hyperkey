using System.Collections.Immutable;

namespace Hyperkey.Core;

public enum TriggerKey
{
    CapsLock,
    ScrollLock
}

public enum OutputModifier
{
    Control,
    Alt,
    Shift
}

public enum TapBehavior
{
    CapsLock
}

public sealed record HyperkeySettings
{
    public const int CurrentSchemaVersion = 1;

    private HyperkeySettings(
        int schemaVersion,
        bool enabled,
        TriggerKey trigger,
        ImmutableArray<OutputModifier> outputModifiers,
        bool launchAtStartup,
        bool launchToTray,
        TapBehavior tapBehavior)
    {
        SchemaVersion = schemaVersion;
        Enabled = enabled;
        Trigger = trigger;
        OutputModifiers = outputModifiers;
        LaunchAtStartup = launchAtStartup;
        LaunchToTray = launchToTray;
        TapBehavior = tapBehavior;
    }

    public int SchemaVersion { get; }

    public bool Enabled { get; }

    public TriggerKey Trigger { get; }

    public ImmutableArray<OutputModifier> OutputModifiers { get; }

    public bool LaunchAtStartup { get; }

    public bool LaunchToTray { get; }

    public TapBehavior TapBehavior { get; }

    public static HyperkeySettings Defaults { get; } = Create(
        enabled: true,
        trigger: TriggerKey.CapsLock,
        outputModifiers: ImmutableArray.Create(
            OutputModifier.Control,
            OutputModifier.Alt,
            OutputModifier.Shift),
        launchAtStartup: true,
        launchToTray: false,
        tapBehavior: TapBehavior.CapsLock);

    public static HyperkeySettings Create(
        bool enabled,
        TriggerKey trigger,
        ImmutableArray<OutputModifier> outputModifiers,
        bool launchAtStartup,
        bool launchToTray,
        TapBehavior tapBehavior,
        int schemaVersion = CurrentSchemaVersion)
    {
        if (schemaVersion != CurrentSchemaVersion)
        {
            throw new ArgumentOutOfRangeException(nameof(schemaVersion));
        }

        if (trigger is not TriggerKey.CapsLock and not TriggerKey.ScrollLock)
        {
            throw new ArgumentOutOfRangeException(nameof(trigger));
        }

        if (tapBehavior != TapBehavior.CapsLock)
        {
            throw new ArgumentOutOfRangeException(nameof(tapBehavior));
        }

        if (outputModifiers.IsDefaultOrEmpty || outputModifiers.Length > DefaultsOutputModifiers.Length)
        {
            throw new ArgumentException(
                "Choose at least one and no more than three output modifiers.",
                nameof(outputModifiers));
        }

        foreach (var modifier in outputModifiers)
        {
            if (modifier is not OutputModifier.Control
                and not OutputModifier.Alt
                and not OutputModifier.Shift)
            {
                throw new ArgumentOutOfRangeException(nameof(outputModifiers), modifier, "Unknown output modifier.");
            }
        }

        if (outputModifiers.Distinct().Count() != outputModifiers.Length)
        {
            throw new ArgumentException("Output modifiers must be unique.", nameof(outputModifiers));
        }

        return new HyperkeySettings(
            schemaVersion,
            enabled,
            trigger,
            outputModifiers,
            launchAtStartup,
            launchToTray,
            tapBehavior);
    }

    public HyperkeySettings WithEnabled(bool enabled) => new(
        SchemaVersion,
        enabled,
        Trigger,
        OutputModifiers,
        LaunchAtStartup,
        LaunchToTray,
        TapBehavior);

    public HyperkeySettings WithLaunchAtStartup(bool launchAtStartup) => new(
        SchemaVersion,
        Enabled,
        Trigger,
        OutputModifiers,
        launchAtStartup,
        LaunchToTray,
        TapBehavior);

    public HyperkeySettings WithLaunchToTray(bool launchToTray) => new(
        SchemaVersion,
        Enabled,
        Trigger,
        OutputModifiers,
        LaunchAtStartup,
        launchToTray,
        TapBehavior);

    public HyperkeySettings WithTrigger(TriggerKey trigger) => Create(
        Enabled,
        trigger,
        OutputModifiers,
        LaunchAtStartup,
        LaunchToTray,
        TapBehavior,
        SchemaVersion);

    public HyperkeySettings WithOutputModifiers(ImmutableArray<OutputModifier> outputModifiers) => Create(
        Enabled,
        Trigger,
        outputModifiers,
        LaunchAtStartup,
        LaunchToTray,
        TapBehavior,
        SchemaVersion);

    private static ImmutableArray<OutputModifier> DefaultsOutputModifiers => ImmutableArray.Create(
        OutputModifier.Control,
        OutputModifier.Alt,
        OutputModifier.Shift);
}
