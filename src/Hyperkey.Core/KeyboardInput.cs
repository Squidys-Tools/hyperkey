namespace Hyperkey.Core;

public enum KeyTransition
{
    Down,
    Up
}

public readonly record struct VirtualKey(ushort Value)
{
    public static VirtualKey CapsLock { get; } = new(0x14);

    public static VirtualKey ScrollLock { get; } = new(0x91);
}

public readonly record struct KeyboardEvent(
    VirtualKey Key,
    KeyTransition Transition,
    bool IsSynthetic = false);
