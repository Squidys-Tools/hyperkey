namespace Hyperkey.Input;

internal enum HookWatchdogAction
{
    SendProbe,
    ReinstallHook
}

/// <summary>
/// Decides when the low-level keyboard hook needs re-installing. The hook
/// callback stamps a heartbeat on every event it sees; a timer-driven probe
/// (a tagged SendInput key-up) keeps beats arriving while the system is
/// otherwise quiet. Windows silently removes low-level hooks that stall, so
/// a heartbeat older than the dead-after threshold is treated as a dead hook
/// and a reinstall is requested on every tick until a fresh heartbeat or
/// <see cref="Reset"/> proves the hook is back.
/// </summary>
internal sealed class HookWatchdog
{
    private readonly long _deadAfterMilliseconds;
    private long _lastHeartbeatTicks;

    public HookWatchdog(long deadAfterMilliseconds, long startTicks)
    {
        _deadAfterMilliseconds = deadAfterMilliseconds;
        _lastHeartbeatTicks = startTicks;
    }

    public void Heartbeat(long nowTicks) => Volatile.Write(ref _lastHeartbeatTicks, nowTicks);

    public void Reset(long nowTicks) => Heartbeat(nowTicks);

    public HookWatchdogAction Tick(long nowTicks) =>
        nowTicks - Volatile.Read(ref _lastHeartbeatTicks) >= _deadAfterMilliseconds
            ? HookWatchdogAction.ReinstallHook
            : HookWatchdogAction.SendProbe;
}
