using System.Collections.Immutable;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Hyperkey.Core;

namespace Hyperkey.Input;

public sealed class ModifierSynthesizer
{
    public const ulong SyntheticInputTag = 0x48595045524B4559;

    private const uint InputKeyboard = 1;
    private const uint KeyEventFlagKeyUp = 0x0002;
    private const uint KeyEventFlagScanCode = 0x0008;
    private const ushort CapsLockScanCode = 0x003A;
    private const ushort ScrollLockScanCode = 0x0046;
    private readonly object _stateGate = new();
    private readonly HashSet<OutputModifier> _pressedModifiers = new();

    public bool TryPress(ImmutableArray<OutputModifier> modifiers, out string? error)
    {
        lock (_stateGate)
        {
            var modifiersToPress = modifiers
                .Where(modifier => !_pressedModifiers.Contains(modifier))
                .ToImmutableArray();
            var inputs = CreateInputs(modifiersToPress, isKeyUp: false);
            var sent = Send(inputs);
            TrackSentPresses(modifiersToPress, sent);

            if (inputs.Length == 0 || sent == inputs.Length)
            {
                error = null;
                return true;
            }

            var win32Error = Marshal.GetLastWin32Error();
            ReleaseTrackedLocked(modifiers);
            error = new Win32Exception(win32Error, "Windows rejected the generated modifier press.").Message;
            return false;
        }
    }

    public bool TryRelease(ImmutableArray<OutputModifier> modifiers, out string? error)
    {
        lock (_stateGate)
        {
            var logicalModifiers = modifiers
                .Where(modifier => _pressedModifiers.Contains(modifier))
                .ToImmutableArray();
            var sendOrder = logicalModifiers.Reverse().ToImmutableArray();
            var inputs = CreateInputs(sendOrder, isKeyUp: false);
            var sent = Send(inputs);
            RemoveSentReleases(sendOrder, sent);

            if (inputs.Length == 0 || sent == inputs.Length)
            {
                error = null;
                return true;
            }

            var win32Error = Marshal.GetLastWin32Error();
            ReleaseTrackedLocked(modifiers);
            error = new Win32Exception(win32Error, "Windows rejected the generated modifier release.").Message;
            return false;
        }
    }

    public void ReleaseAll(ImmutableArray<OutputModifier> modifiers)
    {
        lock (_stateGate)
        {
            ReleaseTrackedLocked(modifiers);
        }
    }

    public bool TryReplayTrigger(TriggerKey triggerKey, out string? error)
    {
        var scanCode = GetTriggerScanCode(triggerKey);
        var inputs = new[]
        {
            CreateInput(scanCode, isKeyUp: false),
            CreateInput(scanCode, isKeyUp: true)
        };

        if (Send(inputs) == inputs.Length)
        {
            error = null;
            return true;
        }

        error = new Win32Exception(
            Marshal.GetLastWin32Error(),
            "Windows rejected the trigger-key tap replay.").Message;
        return false;
    }

    private static INPUT[] CreateInputs(ImmutableArray<OutputModifier> modifiers, bool isKeyUp)
    {
        var orderedModifiers = isKeyUp ? modifiers.Reverse() : modifiers;

        return orderedModifiers
            .Select(modifier => new INPUT
            {
                Type = InputKeyboard,
                Data = new INPUTUNION { Keyboard = CreateKeyboardInput(GetScanCode(modifier), isKeyUp) }
            })
            .ToArray();
    }

    private void TrackSentPresses(ImmutableArray<OutputModifier> modifiers, uint sent)
    {
        var sentCount = (int)Math.Min(sent, (uint)modifiers.Length);
        for (var index = 0; index < sentCount; index++)
        {
            _pressedModifiers.Add(modifiers[index]);
        }
    }

    private void RemoveSentReleases(ImmutableArray<OutputModifier> sendOrder, uint sent)
    {
        var sentCount = (int)Math.Min(sent, (uint)sendOrder.Length);
        for (var index = 0; index < sentCount; index++)
        {
            _pressedModifiers.Remove(sendOrder[index]);
        }
    }

    private void ReleaseTrackedLocked(ImmutableArray<OutputModifier> modifiers)
    {
        var logicalModifiers = modifiers
            .Where(modifier => _pressedModifiers.Contains(modifier))
            .ToImmutableArray();
        var sendOrder = logicalModifiers.Reverse().ToImmutableArray();
        var inputs = CreateInputs(sendOrder, isKeyUp: false);
        if (inputs.Length > 0)
        {
            Send(inputs);
        }

        foreach (var modifier in logicalModifiers)
        {
            _pressedModifiers.Remove(modifier);
        }
    }

    private static INPUT CreateInput(ushort scanCode, bool isKeyUp) => new()
    {
        Type = InputKeyboard,
        Data = new INPUTUNION { Keyboard = CreateKeyboardInput(scanCode, isKeyUp) }
    };

    private static KEYBDINPUT CreateKeyboardInput(ushort scanCode, bool isKeyUp) => new()
    {
        VirtualKey = 0,
        ScanCode = scanCode,
        Flags = KeyEventFlagScanCode | (isKeyUp ? KeyEventFlagKeyUp : 0),
        Time = 0,
        ExtraInfo = new UIntPtr(SyntheticInputTag)
    };

    private static ushort GetScanCode(OutputModifier modifier) => modifier switch
    {
        OutputModifier.Control => 0x1D,
        OutputModifier.Alt => 0x38,
        OutputModifier.Shift => 0x2A,
        _ => throw new ArgumentOutOfRangeException(nameof(modifier))
    };

    private static ushort GetTriggerScanCode(TriggerKey triggerKey) => triggerKey switch
    {
        TriggerKey.CapsLock => CapsLockScanCode,
        TriggerKey.ScrollLock => ScrollLockScanCode,
        _ => throw new ArgumentOutOfRangeException(nameof(triggerKey), triggerKey, "Unknown trigger key.")
    };

    private static uint Send(INPUT[] inputs) => SendInput(
        (uint)inputs.Length,
        inputs,
        Marshal.SizeOf<INPUT>());

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
        public MOUSEINPUT Mouse;

        [FieldOffset(0)]
        public KEYBDINPUT Keyboard;

        [FieldOffset(0)]
        public HARDWAREINPUT Hardware;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MOUSEINPUT
    {
        public int X;
        public int Y;
        public uint MouseData;
        public uint Flags;
        public uint Time;
        public UIntPtr ExtraInfo;
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

    [StructLayout(LayoutKind.Sequential)]
    private struct HARDWAREINPUT
    {
        public uint Message;
        public ushort ParameterLow;
        public ushort ParameterHigh;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint inputCount, INPUT[] inputs, int inputSize);
}
