namespace Hyperkey.Core;

public static class TriggerKeyExtensions
{
    public static VirtualKey ToVirtualKey(this TriggerKey triggerKey) => triggerKey switch
    {
        TriggerKey.CapsLock => VirtualKey.CapsLock,
        TriggerKey.ScrollLock => VirtualKey.ScrollLock,
        _ => throw new ArgumentOutOfRangeException(nameof(triggerKey), triggerKey, "Unknown trigger key.")
    };
}
