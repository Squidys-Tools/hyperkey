using Hyperkey.Input;

namespace Hyperkey.Core.Tests;

internal static class HookWatchdogTests
{
    private const long DeadAfter = 100;

    public static void ProbesWhileHeartbeatsArrive()
    {
        var watchdog = new HookWatchdog(DeadAfter, startTicks: 0);

        AssertEqual(HookWatchdogAction.SendProbe, watchdog.Tick(10));
        watchdog.Heartbeat(10);
        AssertEqual(HookWatchdogAction.SendProbe, watchdog.Tick(50));
        watchdog.Heartbeat(50);
        AssertEqual(HookWatchdogAction.SendProbe, watchdog.Tick(149));
    }

    public static void DeadHookRequestsReinstallUntilHeartbeatResumes()
    {
        var watchdog = new HookWatchdog(DeadAfter, startTicks: 0);

        AssertEqual(HookWatchdogAction.SendProbe, watchdog.Tick(50));
        AssertEqual(HookWatchdogAction.ReinstallHook, watchdog.Tick(100));
        AssertEqual(HookWatchdogAction.ReinstallHook, watchdog.Tick(200));
        watchdog.Heartbeat(250);
        AssertEqual(HookWatchdogAction.SendProbe, watchdog.Tick(300));
    }

    public static void ResetResumesProbingAfterReinstall()
    {
        var watchdog = new HookWatchdog(DeadAfter, startTicks: 0);

        AssertEqual(HookWatchdogAction.ReinstallHook, watchdog.Tick(100));
        watchdog.Reset(200);
        AssertEqual(HookWatchdogAction.SendProbe, watchdog.Tick(250));
        AssertEqual(HookWatchdogAction.ReinstallHook, watchdog.Tick(300));
    }

    public static void HeartbeatBoundaryIsExact()
    {
        var watchdog = new HookWatchdog(DeadAfter, startTicks: 0);

        AssertEqual(HookWatchdogAction.SendProbe, watchdog.Tick(99));
        watchdog.Heartbeat(50);
        AssertEqual(HookWatchdogAction.SendProbe, watchdog.Tick(149));
        AssertEqual(HookWatchdogAction.ReinstallHook, watchdog.Tick(150));
    }

    private static void AssertEqual<T>(T expected, T actual)
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
        {
            throw new InvalidOperationException($"Expected {expected}, got {actual}.");
        }
    }
}
