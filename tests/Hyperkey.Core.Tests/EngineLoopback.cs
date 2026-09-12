using System.Collections.Immutable;
using System.Runtime.InteropServices;
using Hyperkey.Core;
using Hyperkey.Input;

namespace Hyperkey.Core.Tests;

/// <summary>
/// End-to-end coverage for the generalized trigger against the real Windows
/// input stack: a live <see cref="InputEngine"/> plus a second low-level hook
/// that observes exactly what survives the engine. The trigger under test is
/// F24 and the chord key is F13 — both inert everywhere, so the test types
/// nothing observable no matter which window has focus. Uses only
/// process-local hooks, SendInput, and async key state, so it needs no window,
/// no focus, and no text field.
/// </summary>
internal static class EngineLoopback
{
    private const uint WhKeyboardLl = 13;
    private const uint WmKeyDown = 0x0100;
    private const uint WmKeyUp = 0x0101;
    private const uint WmSysKeyDown = 0x0104;
    private const uint WmSysKeyUp = 0x0105;
    private const uint WmQuit = 0x0012;
    private const uint PmRemove = 0x0001;
    private const uint KeyEventFlagKeyUp = 0x0002;

    private const ushort TriggerCode = 0x87; // F24.
    private const ushort ChordCode = 0x86; // F13.

    private static readonly object s_gate = new();
    private static readonly List<string> s_log = new();
    private static bool s_done;
    private static uint s_mainThreadId;
    private static IntPtr s_hookHandle;
    private static LowLevelKeyboardProc? s_hookProc;

    public static void GeneralizedTriggerWorksEndToEndOnTheRealHook()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        if (!TriggerKey.TryCreate(TriggerCode, out var trigger))
        {
            throw new InvalidOperationException("F24 must be a supported trigger for the loopback test.");
        }

        var modifiers = ImmutableArray.Create(OutputModifier.Control, OutputModifier.Alt, OutputModifier.Shift);
        using var engine = new InputEngine(trigger, modifiers);

        lock (s_gate)
        {
            s_log.Clear();
            s_done = false;
        }

        // Windows calls low-level hooks in reverse installation order (most
        // recently installed runs first), so the observation hook is installed
        // BEFORE the engine starts: the engine then runs first, and anything it
        // suppresses never reaches this observer.
        s_mainThreadId = GetCurrentThreadId();
        s_hookProc = TestHookCallback;
        s_hookHandle = SetWindowsHookEx(WhKeyboardLl, s_hookProc, GetModuleHandle(null), 0);
        if (s_hookHandle == IntPtr.Zero)
        {
            throw new InvalidOperationException("The test observation hook could not be installed.");
        }

        try
        {
            if (!engine.Start())
            {
                throw new InvalidOperationException("The engine under test could not be started.");
            }

            var sender = new Thread(SenderMain) { IsBackground = true };
            sender.Start();
            PumpMessages(TimeSpan.FromSeconds(25));
            if (!sender.Join(TimeSpan.FromSeconds(5)))
            {
                throw new InvalidOperationException("The loopback sender thread did not finish.");
            }

            AssertLogSequence();
        }
        finally
        {
            engine.Stop();
            if (s_hookHandle != IntPtr.Zero)
            {
                UnhookWindowsHookEx(s_hookHandle);
                s_hookHandle = IntPtr.Zero;
            }

            ReleaseAllModifiers();
            GC.KeepAlive(s_hookProc);
        }
    }

    private static void SenderMain()
    {
        try
        {
            Thread.Sleep(800);

            // Tap: raw events must vanish, replayed (tagged) pair must arrive.
            SendKey(TriggerCode, isKeyUp: false);
            Thread.Sleep(350);
            SendKey(TriggerCode, isKeyUp: true);
            Thread.Sleep(400);

            // Chord: trigger held while the chord key goes down and up.
            SendKey(TriggerCode, isKeyUp: false);
            Thread.Sleep(350);
            SendKey(ChordCode, isKeyUp: false);
            Thread.Sleep(350);
            RecordAsyncState("mid");
            SendKey(ChordCode, isKeyUp: true);
            Thread.Sleep(250);
            SendKey(TriggerCode, isKeyUp: true);
            Thread.Sleep(400);
            RecordAsyncState("post");

            lock (s_gate)
            {
                s_done = true;
            }
        }
        finally
        {
            PostThreadMessage(s_mainThreadId, WmQuit, UIntPtr.Zero, IntPtr.Zero);
        }
    }

    private static void RecordAsyncState(string phase)
    {
        var control = (GetAsyncKeyState(0x11) & 0x8000) != 0;
        var alt = (GetAsyncKeyState(0x12) & 0x8000) != 0;
        var shift = (GetAsyncKeyState(0x10) & 0x8000) != 0;
        lock (s_gate)
        {
            s_log.Add($"ASYNC-{phase}:Ctrl={control}:Alt={alt}:Shift={shift}");
        }
    }

    private static void AssertLogSequence()
    {
        List<string> log;
        lock (s_gate)
        {
            log = new List<string>(s_log);
        }

        void Require(bool condition, string message)
        {
            if (!condition)
            {
                throw new InvalidOperationException(
                    message + Environment.NewLine + "Observed: " + string.Join(" | ", log));
            }
        }

        // Raw trigger events must never survive the engine.
        Require(!log.Any(line => line == "EVT:DOWN:0x87:R"), "Raw F24 key-down reached the observer; the trigger was not suppressed.");
        Require(!log.Any(line => line == "EVT:UP:0x87:R"), "Raw F24 key-up reached the observer; the trigger was not suppressed.");

        // The tap must come back as a tagged replay pair, in order.
        var replayDown = log.IndexOf("EVT:DOWN:0x87:T");
        var replayUp = log.IndexOf("EVT:UP:0x87:T");
        Require(replayDown >= 0 && replayUp == replayDown + 1, "Expected a tagged F24 down/up replay pair for the tap.");

        // The chord key travels through untouched while the trigger is held.
        // (Modifier presses interleave asynchronously, so only order matters.)
        var chordDown = log.IndexOf("EVT:DOWN:0x86:R");
        var chordUp = log.IndexOf("EVT:UP:0x86:R");
        Require(chordDown >= 0 && chordUp > chordDown, "Expected the raw F13 down/up pair to be forwarded during the chord.");
        Require(chordDown > replayUp, "Expected the chord to happen after the tap replay.");

        // Output modifiers are physically held mid-chord and released after.
        var mid = log.FirstOrDefault(line => line.StartsWith("ASYNC-mid:", StringComparison.Ordinal));
        var post = log.FirstOrDefault(line => line.StartsWith("ASYNC-post:", StringComparison.Ordinal));
        Require(mid == "ASYNC-mid:Ctrl=True:Alt=True:Shift=True", "Expected Ctrl+Alt+Shift to be physically held mid-chord.");
        Require(post == "ASYNC-post:Ctrl=False:Alt=False:Shift=False", "Expected all output modifiers to be released after the chord.");
    }

    private static void PumpMessages(TimeSpan timeout)
    {
        var deadline = DateTime.UtcNow + timeout;
        while (DateTime.UtcNow < deadline)
        {
            var drained = false;
            while (PeekMessage(out var message, IntPtr.Zero, 0, 0, PmRemove))
            {
                drained = true;
                if (message.Message == WmQuit)
                {
                    return;
                }

                TranslateMessage(ref message);
                DispatchMessage(ref message);
            }

            lock (s_gate)
            {
                if (s_done && !drained)
                {
                    return;
                }
            }

            Thread.Sleep(20);
        }

        throw new InvalidOperationException("Timed out waiting for the loopback sender.");
    }

    private static IntPtr TestHookCallback(int code, IntPtr wParam, IntPtr lParam)
    {
        if (code >= 0 && TryGetTransition(unchecked((uint)wParam.ToInt64()), out var transition))
        {
            var nativeEvent = Marshal.PtrToStructure<KBDLLHOOKSTRUCT>(lParam);
            var tagged = nativeEvent.ExtraInfo.ToUInt64() == ModifierSynthesizer.SyntheticInputTag;
            var codeHex = ((ushort)nativeEvent.VirtualKeyCode).ToString("X2");
            lock (s_gate)
            {
                s_log.Add($"EVT:{(transition == KeyTransition.Down ? "DOWN" : "UP")}:0x{codeHex}:{(tagged ? "T" : "R")}");
            }
        }

        return CallNextHookEx(s_hookHandle, code, wParam, lParam);
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

    private static void SendKey(ushort virtualKey, bool isKeyUp)
    {
        var inputs = new[]
        {
            new INPUT
            {
                Type = 1,
                Data = new INPUTUNION
                {
                    Keyboard = new KEYBDINPUT
                    {
                        VirtualKey = virtualKey,
                        ScanCode = 0,
                        Flags = isKeyUp ? KeyEventFlagKeyUp : 0,
                        Time = 0,
                        ExtraInfo = UIntPtr.Zero
                    }
                }
            }
        };

        if (SendInput((uint)inputs.Length, inputs, Marshal.SizeOf<INPUT>()) != inputs.Length)
        {
            lock (s_gate)
            {
                s_log.Add($"SEND-FAILED:0x{virtualKey:X2}:{(isKeyUp ? "UP" : "DOWN")}");
            }
        }
    }

    private static void ReleaseAllModifiers()
    {
        foreach (var code in new ushort[] { 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5 })
        {
            SendKey(code, isKeyUp: true);
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

    [StructLayout(LayoutKind.Sequential)]
    private struct INPUT
    {
        public uint Type;
        public INPUTUNION Data;
    }

    [StructLayout(LayoutKind.Explicit, Size = 32)]
    private struct INPUTUNION
    {
        [FieldOffset(0)]
        public KEYBDINPUT Keyboard;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct KEYBDINPUT
    {
        public ushort VirtualKey;
        public ushort ScanCode;
        public uint Flags;
        public uint Time;
        public UIntPtr ExtraInfo;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SetWindowsHookEx(
        uint hookType,
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

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool TranslateMessage(ref MSG message);

    [DllImport("user32.dll")]
    private static extern IntPtr DispatchMessage(ref MSG message);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool PostThreadMessage(uint threadId, uint message, UIntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint inputCount, INPUT[] inputs, int inputSize);

    [DllImport("user32.dll")]
    private static extern short GetAsyncKeyState(int virtualKey);
}
