using Hyperkey.Input;
using Xunit;

namespace Hyperkey.Core.Tests;

public class HookWatchdogTests
{
    private const long DeadAfter = 100;

    [Fact]
    public void ProbesWhileHeartbeatsArrive()
    {
        var watchdog = new HookWatchdog(DeadAfter, startTicks: 0);

        Assert.Equal(HookWatchdogAction.SendProbe, watchdog.Tick(10));
        watchdog.Heartbeat(10);
        Assert.Equal(HookWatchdogAction.SendProbe, watchdog.Tick(50));
        watchdog.Heartbeat(50);
        Assert.Equal(HookWatchdogAction.SendProbe, watchdog.Tick(149));
    }

    [Fact]
    public void DeadHookRequestsReinstallUntilHeartbeatResumes()
    {
        var watchdog = new HookWatchdog(deadAfterMilliseconds: 250, startTicks: 0);

        Assert.Equal(HookWatchdogAction.SendProbe, watchdog.Tick(100));
        Assert.Equal(HookWatchdogAction.ReinstallHook, watchdog.Tick(250));
        Assert.Equal(HookWatchdogAction.ReinstallHook, watchdog.Tick(500));
        watchdog.Heartbeat(600);
        Assert.Equal(HookWatchdogAction.SendProbe, watchdog.Tick(700));
    }

    [Fact]
    public void ResetResumesProbingAfterReinstall()
    {
        var watchdog = new HookWatchdog(DeadAfter, startTicks: 0);

        Assert.Equal(HookWatchdogAction.ReinstallHook, watchdog.Tick(100));
        watchdog.Reset(200);
        Assert.Equal(HookWatchdogAction.SendProbe, watchdog.Tick(250));
        Assert.Equal(HookWatchdogAction.ReinstallHook, watchdog.Tick(300));
    }

    [Fact]
    public void HeartbeatBoundaryIsExact()
    {
        var watchdog = new HookWatchdog(DeadAfter, startTicks: 0);

        Assert.Equal(HookWatchdogAction.SendProbe, watchdog.Tick(99));
        watchdog.Heartbeat(50);
        Assert.Equal(HookWatchdogAction.SendProbe, watchdog.Tick(149));
        Assert.Equal(HookWatchdogAction.ReinstallHook, watchdog.Tick(150));
    }
}
