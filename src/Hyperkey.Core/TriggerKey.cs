namespace Hyperkey.Core;

/// <summary>
/// Identifies the physical key that activates the Hyperkey modifier layer.
/// Wraps a Win32 virtual-key code so the trigger can be rebound to any normal
/// keyboard key (letters, digits, function keys, punctuation, and a few
/// lock keys), not just Caps Lock or Scroll Lock.
/// </summary>
/// <remarks>
/// Only keys from the explicit allowlist can be constructed: keys that the
/// low-level hook can reliably suppress and that <see cref="TriggerKey"/>
/// tap-replay can synthesize back through a plain (non-extended) scan code.
/// Excluded classes, with the technical reason each is unsafe:
/// <list type="bullet">
/// <item>Modifier keys (Shift, Ctrl, Alt, Win, left and right): the engine
/// presses and releases these itself as output, so they cannot also be the trigger.</item>
/// <item>Escape: reserved as the rebind-cancel key in the settings UI.</item>
/// <item>Extended-scan-code keys (arrows, Home/End, Insert/Delete, PageUp/PageDown):
/// tap-replay synthesizes a plain scan code, which would replay the wrong key.</item>
/// <item>Windows/Apps keys: suppressing them breaks Start-menu and system chords.</item>
/// <item>Media, browser, launcher, Sleep, and OEM-specific keys: these travel
/// as app-command or vendor paths the hook cannot reliably suppress and replay.</item>
/// <item>Snapshot, Pause, IME, Packet, and reserved codes: multi-byte or
/// synthetic paths that cannot be replayed through a single scan code.</item>
/// </list>
/// </remarks>
public readonly record struct TriggerKey
{
    private static readonly (ushort Code, string Wire, string Label)[] KnownKeys =
    {
        (0x08, "Back", "Backspace"),
        (0x09, "Tab", "Tab"),
        (0x0D, "Enter", "Enter"),
        (0x14, "CapsLock", "Caps Lock"),
        (0x20, "Space", "Space"),
        (0x30, "D0", "0"),
        (0x31, "D1", "1"),
        (0x32, "D2", "2"),
        (0x33, "D3", "3"),
        (0x34, "D4", "4"),
        (0x35, "D5", "5"),
        (0x36, "D6", "6"),
        (0x37, "D7", "7"),
        (0x38, "D8", "8"),
        (0x39, "D9", "9"),
        (0x41, "A", "A"),
        (0x42, "B", "B"),
        (0x43, "C", "C"),
        (0x44, "D", "D"),
        (0x45, "E", "E"),
        (0x46, "F", "F"),
        (0x47, "G", "G"),
        (0x48, "H", "H"),
        (0x49, "I", "I"),
        (0x4A, "J", "J"),
        (0x4B, "K", "K"),
        (0x4C, "L", "L"),
        (0x4D, "M", "M"),
        (0x4E, "N", "N"),
        (0x4F, "O", "O"),
        (0x50, "P", "P"),
        (0x51, "Q", "Q"),
        (0x52, "R", "R"),
        (0x53, "S", "S"),
        (0x54, "T", "T"),
        (0x55, "U", "U"),
        (0x56, "V", "V"),
        (0x57, "W", "W"),
        (0x58, "X", "X"),
        (0x59, "Y", "Y"),
        (0x5A, "Z", "Z"),
        (0x60, "NumPad0", "NumPad0"),
        (0x61, "NumPad1", "NumPad1"),
        (0x62, "NumPad2", "NumPad2"),
        (0x63, "NumPad3", "NumPad3"),
        (0x64, "NumPad4", "NumPad4"),
        (0x65, "NumPad5", "NumPad5"),
        (0x66, "NumPad6", "NumPad6"),
        (0x67, "NumPad7", "NumPad7"),
        (0x68, "NumPad8", "NumPad8"),
        (0x69, "NumPad9", "NumPad9"),
        (0x6A, "NumPadMultiply", "Num *"),
        (0x6B, "NumPadAdd", "Num +"),
        (0x6C, "NumPadSeparator", "Num Sep"),
        (0x6D, "NumPadSubtract", "Num -"),
        (0x6E, "NumPadDecimal", "Num ."),
        (0x70, "F1", "F1"),
        (0x71, "F2", "F2"),
        (0x72, "F3", "F3"),
        (0x73, "F4", "F4"),
        (0x74, "F5", "F5"),
        (0x75, "F6", "F6"),
        (0x76, "F7", "F7"),
        (0x77, "F8", "F8"),
        (0x78, "F9", "F9"),
        (0x79, "F10", "F10"),
        (0x7A, "F11", "F11"),
        (0x7B, "F12", "F12"),
        (0x7C, "F13", "F13"),
        (0x7D, "F14", "F14"),
        (0x7E, "F15", "F15"),
        (0x7F, "F16", "F16"),
        (0x80, "F17", "F17"),
        (0x81, "F18", "F18"),
        (0x82, "F19", "F19"),
        (0x83, "F20", "F20"),
        (0x84, "F21", "F21"),
        (0x85, "F22", "F22"),
        (0x86, "F23", "F23"),
        (0x87, "F24", "F24"),
        (0x90, "NumLock", "Num Lock"),
        (0x91, "ScrollLock", "Scroll Lock"),
        (0xBA, "OemSemicolon", ";"),
        (0xBB, "OemPlus", "="),
        (0xBC, "OemComma", ","),
        (0xBD, "OemMinus", "-"),
        (0xBE, "OemPeriod", "."),
        (0xBF, "OemQuestion", "/"),
        (0xC0, "OemTilde", "`"),
        (0xDB, "OemOpenBrackets", "["),
        (0xDC, "OemBackslash", "\\"),
        (0xDD, "OemCloseBrackets", "]"),
        (0xDE, "OemQuotes", "'"),
        (0xDF, "Oem8", "Oem8"),
    };

    private TriggerKey(ushort virtualKeyCode)
    {
        VirtualKeyCode = virtualKeyCode;
    }

    public static TriggerKey CapsLock { get; } = new(0x14);

    public static TriggerKey ScrollLock { get; } = new(0x91);

    public ushort VirtualKeyCode { get; }

    public VirtualKey ToVirtualKey() => new(VirtualKeyCode);

    public bool IsSupported => IsSupportedKey(VirtualKeyCode);

    public string WireName => TryFind(VirtualKeyCode, out var known)
        ? known.Wire
        : $"Unknown(0x{VirtualKeyCode:X2})";

    public string DisplayLabel => TryFind(VirtualKeyCode, out var known)
        ? known.Label
        : $"Unknown (0x{VirtualKeyCode:X2})";

    public static bool IsSupportedKey(ushort virtualKeyCode) =>
        TryFind(virtualKeyCode, out _);

    public static bool TryCreate(ushort virtualKeyCode, out TriggerKey triggerKey)
    {
        if (TryFind(virtualKeyCode, out _))
        {
            triggerKey = new TriggerKey(virtualKeyCode);
            return true;
        }

        triggerKey = default;
        return false;
    }

    public static bool TryParse(string? wireName, out TriggerKey triggerKey)
    {
        if (!string.IsNullOrWhiteSpace(wireName))
        {
            foreach (var known in KnownKeys)
            {
                if (string.Equals(known.Wire, wireName, StringComparison.OrdinalIgnoreCase))
                {
                    triggerKey = new TriggerKey(known.Code);
                    return true;
                }
            }
        }

        triggerKey = default;
        return false;
    }

    public override string ToString() => DisplayLabel;

    private static bool TryFind(
        ushort virtualKeyCode,
        out (ushort Code, string Wire, string Label) known)
    {
        foreach (var candidate in KnownKeys)
        {
            if (candidate.Code == virtualKeyCode)
            {
                known = candidate;
                return true;
            }
        }

        known = default;
        return false;
    }
}
