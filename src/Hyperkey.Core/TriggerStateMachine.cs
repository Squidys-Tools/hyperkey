using System.Collections.Immutable;

namespace Hyperkey.Core;

public enum TriggerPhase
{
    Idle,
    TriggerHeld,
    HyperActive
}

public sealed record TriggerMachineState(TriggerPhase Phase)
{
    public static TriggerMachineState Idle { get; } = new(TriggerPhase.Idle);
}

public abstract record InputDecision
{
    private InputDecision()
    {
    }

    public sealed record PassThrough : InputDecision;

    public sealed record Suppress : InputDecision;

    public sealed record Forward : InputDecision;

    public sealed record PressAndForward(ImmutableArray<OutputModifier> Modifiers) : InputDecision;

    public sealed record ReleaseAndSuppress(ImmutableArray<OutputModifier> Modifiers) : InputDecision;

    public sealed record ReplayTrigger : InputDecision;
}

public sealed record TriggerTransition(
    TriggerMachineState State,
    InputDecision Decision);

public static class TriggerStateMachine
{
    public static TriggerTransition Process(
        TriggerMachineState current,
        KeyboardEvent input,
        ImmutableArray<OutputModifier> outputModifiers) =>
        Process(current, input, TriggerKey.CapsLock, outputModifiers);

    public static TriggerTransition Process(
        TriggerMachineState current,
        KeyboardEvent input,
        TriggerKey triggerKey,
        ImmutableArray<OutputModifier> outputModifiers)
    {
        if (input.IsSynthetic)
        {
            return new TriggerTransition(current, new InputDecision.PassThrough());
        }

        var isTrigger = input.Key == triggerKey.ToVirtualKey();
        return current.Phase switch
        {
            TriggerPhase.Idle => ProcessIdle(input, isTrigger),
            TriggerPhase.TriggerHeld => ProcessTriggerHeld(input, isTrigger, outputModifiers),
            TriggerPhase.HyperActive => ProcessHyperActive(input, isTrigger, outputModifiers),
            _ => throw new ArgumentOutOfRangeException(nameof(current), current.Phase, "Unknown trigger phase.")
        };
    }

    private static TriggerTransition ProcessIdle(KeyboardEvent input, bool isTrigger)
    {
        if (isTrigger && input.Transition == KeyTransition.Down)
        {
            return new TriggerTransition(
                new TriggerMachineState(TriggerPhase.TriggerHeld),
                new InputDecision.Suppress());
        }

        return new TriggerTransition(TriggerMachineState.Idle, new InputDecision.PassThrough());
    }

    private static TriggerTransition ProcessTriggerHeld(
        KeyboardEvent input,
        bool isTrigger,
        ImmutableArray<OutputModifier> outputModifiers)
    {
        if (isTrigger && input.Transition == KeyTransition.Up)
        {
            return new TriggerTransition(
                TriggerMachineState.Idle,
                new InputDecision.ReplayTrigger());
        }

        if (isTrigger)
        {
            return new TriggerTransition(
                new TriggerMachineState(TriggerPhase.TriggerHeld),
                new InputDecision.Suppress());
        }

        if (input.Transition == KeyTransition.Down)
        {
            return new TriggerTransition(
                new TriggerMachineState(TriggerPhase.HyperActive),
                new InputDecision.PressAndForward(outputModifiers));
        }

        return new TriggerTransition(
            new TriggerMachineState(TriggerPhase.TriggerHeld),
            new InputDecision.PassThrough());
    }

    private static TriggerTransition ProcessHyperActive(
        KeyboardEvent input,
        bool isTrigger,
        ImmutableArray<OutputModifier> outputModifiers)
    {
        if (isTrigger && input.Transition == KeyTransition.Up)
        {
            return new TriggerTransition(
                TriggerMachineState.Idle,
                new InputDecision.ReleaseAndSuppress(outputModifiers));
        }

        if (isTrigger)
        {
            return new TriggerTransition(
                new TriggerMachineState(TriggerPhase.HyperActive),
                new InputDecision.Suppress());
        }

        return new TriggerTransition(
            new TriggerMachineState(TriggerPhase.HyperActive),
            new InputDecision.Forward());
    }
}
