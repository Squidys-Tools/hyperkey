using System.Collections.Immutable;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Hyperkey.Core;

namespace Hyperkey.Input;

public enum InputEngineStatus
{
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed
}

public sealed class InputEngine : IDisposable
{
    private const int WhKeyboardLl = 13;
    private const uint WmKeyDown = 0x0100;
    private const uint WmKeyUp = 0x0101;
    private const uint WmSysKeyDown = 0x0104;
    private const uint WmSysKeyUp = 0x0105;
    private const uint WmQuit = 0x0012;
    private const uint WmUser = 0x0400;
    private const uint WmReinstallHook = WmUser + 0x2A;
    private const uint PmNoRemove = 0x0000;
    private const int WatchdogIntervalMilliseconds = 5000;
    private const int WatchdogDeadAfterMilliseconds = WatchdogIntervalMilliseconds * 3;
    private const long HeartbeatTagSigned = unchecked((long)ModifierSynthesizer.HeartbeatInputTag);

    private static readonly int ExtraInfoOffset =
        (int)Marshal.OffsetOf<KBDLLHOOKSTRUCT>(nameof(KBDLLHOOKSTRUCT.ExtraInfo));

    private readonly object _lifecycleGate = new();
    private readonly object _stateGate = new();
    private readonly ModifierSynthesizer _modifierSynthesizer = new();
    private readonly LowLevelKeyboardProc _hookCallback;
    private readonly ManualResetEventSlim _hookReady = new(false);
    private ImmutableArray<OutputModifier> _outputModifiers;
    private Thread? _hookThread;
    private uint _hookThreadId;
    private TriggerKey _triggerKey;
    private IntPtr _hookHandle;
    private volatile bool _stopRequested;
    private TriggerMachineState _state = TriggerMachineState.Idle;
    private InputEngineStatus _status = InputEngineStatus.Stopped;
    private string? _statusError;
    private HookWatchdog? _watchdog;
    private System.Threading.Timer? _watchdogTimer;
    private string? _lastRecoveryMessage;
    private bool _disposed;

    public InputEngine(TriggerKey triggerKey, ImmutableArray<OutputModifier> outputModifiers)
    {
        _triggerKey = triggerKey;
        _outputModifiers = outputModifiers;
        _hookCallback = HookCallback;
    }

    public void Configure(TriggerKey triggerKey, ImmutableArray<OutputModifier> outputModifiers)
    {
        if (Volatile.Read(ref _disposed))
        {
            throw new ObjectDisposedException(nameof(InputEngine));
        }

        lock (_stateGate)
        {
            _modifierSynthesizer.ReleaseAll(_outputModifiers);
            _state = TriggerMachineState.Idle;
            _triggerKey = triggerKey;
            _outputModifiers = outputModifiers;
        }
    }

    public InputEngineStatus Status
    {
        get
        {
            lock (_lifecycleGate)
            {
                return _status;
            }
        }
    }

    public string? StatusError
    {
        get
        {
            lock (_lifecycleGate)
            {
                return _statusError;
            }
        }
    }

    public string? LastRecoveryMessage
    {
        get
        {
            lock (_lifecycleGate)
            {
                return _lastRecoveryMessage;
            }
        }
    }

    public event Action<InputEngineStatus, string?>? StatusChanged;

    public event Action<string>? HookRecovered;

    public bool Start()
    {
        lock (_lifecycleGate)
        {
            ThrowIfDisposed();

            if (_status == InputEngineStatus.Running)
            {
                return true;
            }

            if (_status == InputEngineStatus.Starting || _status == InputEngineStatus.Stopping)
            {
                return false;
            }

            if (_hookThread is not null)
            {
                if (_hookThread.IsAlive)
                {
                    return false;
                }

                _hookThread = null;
                Volatile.Write(ref _hookThreadId, 0);
            }

            _stopRequested = false;
            _hookReady.Reset();
            Volatile.Write(ref _hookThreadId, 0);
            SetStatusLocked(InputEngineStatus.Starting, null);

            _hookThread = new Thread(HookThreadMain)
            {
                IsBackground = true,
                Name = "Hyperkey keyboard hook"
            };
            _hookThread.Start();
        }

        if (!_hookReady.Wait(TimeSpan.FromSeconds(3)))
        {
            Stop();
            return false;
        }

        if (Status != InputEngineStatus.Running)
        {
            return false;
        }

        StartWatchdog();
        return true;
    }

    public void Stop()
    {
        StopWatchdog();

        Thread? hookThread = null;
        uint hookThreadId = 0;
        var alreadyStopped = false;

        lock (_lifecycleGate)
        {
            if (_status == InputEngineStatus.Stopped && _hookThread is null)
            {
                alreadyStopped = true;
            }
            else
            {
                _stopRequested = true;
                SetStatusLocked(InputEngineStatus.Stopping, null);
                hookThread = _hookThread;
                hookThreadId = Volatile.Read(ref _hookThreadId);
            }
        }

        if (alreadyStopped)
        {
            EmergencyDisable();
            return;
        }

        PublishStatus(InputEngineStatus.Stopping, null);

        EmergencyDisable();

        var quitPosted = hookThreadId == 0
            || PostThreadMessage(hookThreadId, WmQuit, UIntPtr.Zero, IntPtr.Zero);

        if (hookThreadId != 0 && !quitPosted)
        {
            PublishStatus(
                InputEngineStatus.Stopping,
                new Win32Exception(
                    Marshal.GetLastWin32Error(),
                    "The keyboard hook shutdown message could not be posted.").Message);
        }

        if (hookThread is not null && hookThread != Thread.CurrentThread)
        {
            hookThread.Join(TimeSpan.FromSeconds(2));
        }

        InputEngineStatus finalStatus;
        string? finalError;
        lock (_lifecycleGate)
        {
            if (_hookThread is null || !_hookThread.IsAlive)
            {
                _hookThread = null;
                _hookThreadId = 0;
                SetStatusLocked(InputEngineStatus.Stopped, null);
            }
            else
            {
                SetStatusLocked(InputEngineStatus.Failed, "The keyboard hook thread did not stop cleanly.");
            }

            finalStatus = _status;
            finalError = _statusError;
        }

        PublishStatus(finalStatus, finalError);
    }

    public void EmergencyDisable()
    {
        lock (_stateGate)
        {
            _state = TriggerMachineState.Idle;
            _modifierSynthesizer.ReleaseAll(_outputModifiers);
        }
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        Stop();
        _hookReady.Dispose();
        Volatile.Write(ref _disposed, true);
    }

    private void HookThreadMain()
    {
        var installed = false;
        try
        {
            Volatile.Write(ref _hookThreadId, GetCurrentThreadId());
            PeekMessage(out _, IntPtr.Zero, 0, 0, PmNoRemove);

            if (_stopRequested)
            {
                _hookReady.Set();
                return;
            }

            _hookHandle = SetWindowsHookEx(
                WhKeyboardLl,
                _hookCallback,
                GetModuleHandle(null),
                0);

            if (_hookHandle == IntPtr.Zero)
            {
                var error = new Win32Exception(Marshal.GetLastWin32Error(), "The low-level keyboard hook could not be installed.").Message;
                SetStatus(InputEngineStatus.Failed, error);
                _hookReady.Set();
                return;
            }

            installed = true;
            SetStatus(InputEngineStatus.Running, null);
            _hookReady.Set();

            while (!_stopRequested)
            {
                var result = GetMessage(out var message, IntPtr.Zero, 0, 0);
                if (result <= 0)
                {
                    if (result < 0 && !_stopRequested)
                    {
                        SetStatus(InputEngineStatus.Failed, "The keyboard hook message loop failed.");
                    }

                    break;
                }

                if (message.Message == WmReinstallHook)
                {
                    if (!ReinstallHook())
                    {
                        break;
                    }

                    continue;
                }

                TranslateMessage(ref message);
                DispatchMessage(ref message);
            }
        }
        catch (Exception exception)
        {
            if (!_stopRequested)
            {
                SetStatus(InputEngineStatus.Failed, $"The keyboard hook thread failed: {exception.Message}");
            }

            _hookReady.Set();
        }
        finally
        {
            if (installed)
            {
                UnhookWindowsHookEx(_hookHandle);
            }

            _hookHandle = IntPtr.Zero;
            EmergencyDisable();

            lock (_lifecycleGate)
            {
                Volatile.Write(ref _hookThreadId, 0);
                if (!_stopRequested && _status != InputEngineStatus.Failed)
                {
                    SetStatusLocked(InputEngineStatus.Failed, "The keyboard hook stopped unexpectedly.");
                }
            }

            _hookReady.Set();
        }
    }

    private IntPtr HookCallback(int code, IntPtr wParam, IntPtr lParam)
    {
        if (code < 0)
        {
            return CallNextHookEx(_hookHandle, code, wParam, lParam);
        }

        Volatile.Read(ref _watchdog)?.Heartbeat(Environment.TickCount64);
        if (Marshal.ReadIntPtr(lParam, ExtraInfoOffset).ToInt64() == HeartbeatTagSigned)
        {
            // A watchdog probe only proves the hook is still alive; suppress it
            // so the beat never reaches the rest of the system.
            return new IntPtr(1);
        }

        try
        {
            if (!TryGetTransition(unchecked((uint)wParam.ToInt64()), out var transition))
            {
                return CallNextHookEx(_hookHandle, code, wParam, lParam);
            }

            var nativeEvent = Marshal.PtrToStructure<KBDLLHOOKSTRUCT>(lParam);
            var input = new KeyboardEvent(
                new VirtualKey((ushort)nativeEvent.VirtualKeyCode),
                transition,
                nativeEvent.ExtraInfo.ToUInt64() == ModifierSynthesizer.SyntheticInputTag);

            lock (_stateGate)
            {
                var result = TriggerStateMachine.Process(_state, input, _triggerKey, _outputModifiers);
                _state = result.State;

                return result.Decision switch
                {
                    InputDecision.PassThrough => CallNextHookEx(_hookHandle, code, wParam, lParam),
                    InputDecision.Forward => CallNextHookEx(_hookHandle, code, wParam, lParam),
                    InputDecision.Suppress => new IntPtr(1),
                    InputDecision.PressAndForward press => PressModifiersAndForward(press, code, wParam, lParam),
                    InputDecision.ReleaseAndSuppress release => ReleaseModifiersAndSuppress(release),
                    InputDecision.ReplayTrigger => ReplayTriggerAndSuppress(_triggerKey),
                    _ => throw new ArgumentOutOfRangeException(nameof(result.Decision))
                };
            }
        }
        catch (Exception exception)
        {
            EmergencyDisable();
            SetStatus(InputEngineStatus.Failed, $"The keyboard hook failed: {exception.Message}");
            return CallNextHookEx(_hookHandle, code, wParam, lParam);
        }
    }

    private IntPtr PressModifiersAndForward(
        InputDecision.PressAndForward press,
        int code,
        IntPtr wParam,
        IntPtr lParam)
    {
        if (_modifierSynthesizer.TryPress(press.Modifiers, out var error))
        {
            return CallNextHookEx(_hookHandle, code, wParam, lParam);
        }

        _state = TriggerMachineState.Idle;
        SetStatus(InputEngineStatus.Failed, error ?? "The modifier layer could not be activated.");
        return new IntPtr(1);
    }

    private IntPtr ReleaseModifiersAndSuppress(InputDecision.ReleaseAndSuppress release)
    {
        _modifierSynthesizer.TryRelease(release.Modifiers, out var error);
        if (error is not null)
        {
            SetStatus(InputEngineStatus.Failed, error);
        }

        return new IntPtr(1);
    }

    private IntPtr ReplayTriggerAndSuppress(TriggerKey triggerKey)
    {
        if (!_modifierSynthesizer.TryReplayTrigger(triggerKey, out var error))
        {
            SetStatus(InputEngineStatus.Failed, error ?? "The trigger-key tap could not be replayed.");
        }

        // The physical key-down was already suppressed, so the physical key-up must
        // stay suppressed even if Windows rejects the replay.
        return new IntPtr(1);
    }

    private void StartWatchdog()
    {
        StopWatchdog();

        lock (_lifecycleGate)
        {
            if (Volatile.Read(ref _disposed) || _stopRequested)
            {
                return;
            }

            _watchdog = new HookWatchdog(WatchdogDeadAfterMilliseconds, Environment.TickCount64);
            _watchdogTimer = new System.Threading.Timer(
                WatchdogTick,
                null,
                WatchdogIntervalMilliseconds,
                WatchdogIntervalMilliseconds);
        }
    }

    private void StopWatchdog()
    {
        System.Threading.Timer? timer;
        lock (_lifecycleGate)
        {
            timer = _watchdogTimer;
            _watchdogTimer = null;
            _watchdog = null;
        }

        timer?.Dispose();
    }

    private void WatchdogTick(object? state)
    {
        if (Volatile.Read(ref _disposed) || _stopRequested || Status != InputEngineStatus.Running)
        {
            return;
        }

        var watchdog = Volatile.Read(ref _watchdog);
        if (watchdog is null)
        {
            return;
        }

        switch (watchdog.Tick(Environment.TickCount64))
        {
            case HookWatchdogAction.SendProbe:
                _modifierSynthesizer.TrySendHeartbeatProbe();
                break;
            case HookWatchdogAction.ReinstallHook:
                // The hook can only be uninstalled and reinstalled on the thread
                // that owns the message loop, so hand the work to it.
                var hookThreadId = Volatile.Read(ref _hookThreadId);
                var posted = hookThreadId != 0
                    && PostThreadMessage(hookThreadId, WmReinstallHook, UIntPtr.Zero, IntPtr.Zero);
                if (!posted)
                {
                    watchdog.Reset(Environment.TickCount64);
                }

                break;
        }
    }

    private bool ReinstallHook()
    {
        var watchdog = Volatile.Read(ref _watchdog);
        if (_stopRequested || watchdog is null)
        {
            return true;
        }

        // Reinstall requests can pile up while the hook thread is stalled. A
        // heartbeat that arrived after this message was posted proves the hook
        // is alive again, so a queued leftover has nothing to repair.
        if (watchdog.Tick(Environment.TickCount64) == HookWatchdogAction.SendProbe)
        {
            return true;
        }

        if (_hookHandle != IntPtr.Zero)
        {
            UnhookWindowsHookEx(_hookHandle);
            _hookHandle = IntPtr.Zero;
        }

        EmergencyDisable();

        _hookHandle = SetWindowsHookEx(
            WhKeyboardLl,
            _hookCallback,
            GetModuleHandle(null),
            0);

        if (_hookHandle == IntPtr.Zero)
        {
            var error = new Win32Exception(
                Marshal.GetLastWin32Error(),
                "The low-level keyboard hook could not be reinstalled.").Message;
            SetStatus(InputEngineStatus.Failed, error);
            return false;
        }

        watchdog.Reset(Environment.TickCount64);
        RecordRecovery(
            $"The keyboard hook was reinstalled automatically at {DateTime.Now:T} after it stopped responding.");
        return true;
    }

    private void RecordRecovery(string message)
    {
        lock (_lifecycleGate)
        {
            _lastRecoveryMessage = message;
        }

        var handler = HookRecovered;
        if (handler is null)
        {
            return;
        }

        try
        {
            handler(message);
        }
        catch
        {
            // A recovery observer must never break the hook thread.
        }
    }

    private void SetStatus(InputEngineStatus status, string? error)
    {
        lock (_lifecycleGate)
        {
            SetStatusLocked(status, error);
        }

        PublishStatus(status, error);
    }

    private void SetStatusLocked(InputEngineStatus status, string? error)
    {
        _status = status;
        _statusError = error;
    }

    private void PublishStatus(InputEngineStatus status, string? error)
    {
        var handler = StatusChanged;
        if (handler is null)
        {
            return;
        }

        try
        {
            handler(status, error);
        }
        catch
        {
            // A status observer must never break the hook thread.
        }
    }

    private static bool TryGetTransition(uint message, out KeyTransition transition)
    {
        switch (message)
        {
            case WmKeyDown:
            case WmSysKeyDown:
                transition = KeyTransition.Down;
                return true;
            case WmKeyUp:
            case WmSysKeyUp:
                transition = KeyTransition.Up;
                return true;
            default:
                transition = default;
                return false;
        }
    }

    private void ThrowIfDisposed()
    {
        if (_disposed)
        {
            throw new ObjectDisposedException(nameof(InputEngine));
        }
    }

    [UnmanagedFunctionPointer(CallingConvention.Winapi)]
    private delegate IntPtr LowLevelKeyboardProc(int code, IntPtr wParam, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential)]
    private struct KBDLLHOOKSTRUCT
    {
        public uint VirtualKeyCode;
        public uint ScanCode;
        public uint Flags;
        public uint Time;
        public UIntPtr ExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT
    {
        public int X;
        public int Y;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MSG
    {
        public IntPtr WindowHandle;
        public uint Message;
        public IntPtr WParam;
        public IntPtr LParam;
        public uint Time;
        public POINT Point;
        public uint Private;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SetWindowsHookEx(
        int hookType,
        LowLevelKeyboardProc callback,
        IntPtr moduleHandle,
        uint threadId);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool UnhookWindowsHookEx(IntPtr hookHandle);

    [DllImport("user32.dll")]
    private static extern IntPtr CallNextHookEx(IntPtr hookHandle, int code, IntPtr wParam, IntPtr lParam);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr GetModuleHandle(string? moduleName);

    [DllImport("kernel32.dll")]
    private static extern uint GetCurrentThreadId();

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool PeekMessage(out MSG message, IntPtr windowHandle, uint minimumMessage, uint maximumMessage, uint removeMessage);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern int GetMessage(out MSG message, IntPtr windowHandle, uint minimumMessage, uint maximumMessage);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool TranslateMessage(ref MSG message);

    [DllImport("user32.dll")]
    private static extern IntPtr DispatchMessage(ref MSG message);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool PostThreadMessage(uint threadId, uint message, UIntPtr wParam, IntPtr lParam);
}
