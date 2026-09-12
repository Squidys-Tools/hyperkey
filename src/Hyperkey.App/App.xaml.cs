using Hyperkey.App.Services;
using Hyperkey.Core;
using Hyperkey.Input;
using Microsoft.Win32;
using System.Windows;
using System.Windows.Media;
using Wpf.Ui.Appearance;

namespace Hyperkey.App;

public partial class App : Application
{
    private readonly SingleInstanceService _singleInstance;
    private readonly SettingsStore _settingsStore;
    private readonly StartupRegistrationService _startupRegistrationService;
    private readonly InputEngine _inputEngine;
    private MainWindow? _window;
    private TrayIconService? _trayIcon;
    private HyperkeySettings _settings = HyperkeySettings.Defaults;
    private string? _recoveryMessage;
    private string? _diagnosticMessage;
    private string? _startupRegistrationError;
    private bool _sessionLocked;
    private bool _isQuitting;

    public App()
    {
        InitializeComponent();

        _settingsStore = new SettingsStore();
        _startupRegistrationService = new StartupRegistrationService();
        _inputEngine = new InputEngine(
            HyperkeySettings.Defaults.Trigger,
            HyperkeySettings.Defaults.OutputModifiers);
        _inputEngine.StatusChanged += OnInputEngineStatusChanged;
        _singleInstance = new SingleInstanceService();
        if (!_singleInstance.TryAcquire())
        {
            Environment.Exit(0);
            return;
        }
    }

    public static App CurrentApp => (App)Current;

    public HyperkeySettings Settings => _settings;

    public string? RecoveryMessage => _recoveryMessage;

    public string? DiagnosticMessage => _startupRegistrationError ?? _diagnosticMessage;

    public string DiagnosticDetails => string.Join(
        Environment.NewLine,
        $"Hyperkey version: {AppVersion.Display}",
        $"Enabled: {_settings.Enabled}",
        $"Trigger: {_settings.Trigger}",
        $"Output modifiers: {string.Join(", ", _settings.OutputModifiers)}",
        $"Input status: {_inputEngine.Status}",
        $"Input status error: {InputStatusError ?? "None"}",
        $"Launch at login: {_settings.LaunchAtStartup}",
        $"Launch to tray: {_settings.LaunchToTray}",
        $"Diagnostic message: {DiagnosticMessage ?? "None"}",
        $"Recovery message: {_recoveryMessage ?? "None"}");

    public InputEngineStatus InputStatus => _inputEngine.Status;

    public string? InputStatusError { get; private set; }

    public event Action<HyperkeySettings>? SettingsChanged;

    public event Action<InputEngineStatus, string?>? InputStatusChanged;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        ApplicationThemeManager.ApplySystemTheme();
        ApplyHyperkeyAccent();
        SystemEvents.PowerModeChanged += OnPowerModeChanged;
        SystemEvents.SessionSwitch += OnSessionSwitch;

        var loadResult = _settingsStore.Load();
        _settings = loadResult.Settings;
        _recoveryMessage = loadResult.Message;
        _inputEngine.Configure(_settings.Trigger, _settings.OutputModifiers);
        ApplyStartupRegistration(_settings.LaunchAtStartup);

        if (_settings.Enabled && !_inputEngine.Start())
        {
            _diagnosticMessage = _inputEngine.StatusError ?? "The keyboard hook could not be started.";
        }

        if (!_settings.LaunchToTray)
        {
            EnsureSettingsWindow().ShowSettings();
        }

        _trayIcon = new TrayIconService(
            _settings,
            ToggleEnabled,
            OpenSettings,
            Quit);

        if (!_trayIcon.IsRegistered)
        {
            _diagnosticMessage = "The system tray icon could not be registered.";
        }

        RefreshUi();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        SystemEvents.PowerModeChanged -= OnPowerModeChanged;
        SystemEvents.SessionSwitch -= OnSessionSwitch;
        _trayIcon?.Dispose();
        _inputEngine.EmergencyDisable();
        _inputEngine.Dispose();
        _singleInstance.Dispose();
        base.OnExit(e);
    }

    public void UpdateSettings(HyperkeySettings settings, bool persist = true)
    {
        var inputConfigurationChanged = _settings.Trigger != settings.Trigger
            || !_settings.OutputModifiers.SequenceEqual(settings.OutputModifiers);
        var launchAtStartupChanged = _settings.LaunchAtStartup != settings.LaunchAtStartup;
        _settings = settings;

        if (persist)
        {
            var saveResult = _settingsStore.Save(settings);
            _recoveryMessage = saveResult.Succeeded ? null : saveResult.Error;
        }

        if (launchAtStartupChanged)
        {
            ApplyStartupRegistration(settings.LaunchAtStartup);
        }

        if (inputConfigurationChanged)
        {
            _inputEngine.Configure(settings.Trigger, settings.OutputModifiers);
        }

        if (settings.Enabled)
        {
            if (!_inputEngine.Start())
            {
                _diagnosticMessage = _inputEngine.StatusError ?? "The keyboard hook could not be started.";
            }
            else
            {
                _diagnosticMessage = null;
            }
        }
        else
        {
            _inputEngine.Stop();
            _diagnosticMessage = null;
        }

        RefreshUi();
        SettingsChanged?.Invoke(settings);
    }

    public void RestartInputEngine()
    {
        if (!_settings.Enabled)
        {
            _diagnosticMessage = "Enable Hyperkey before restarting the keyboard hook.";
            RefreshUi();
            return;
        }

        _inputEngine.Stop();
        if (_inputEngine.Start())
        {
            _diagnosticMessage = "The keyboard hook was restarted successfully.";
        }
        else
        {
            _diagnosticMessage = _inputEngine.StatusError ?? "The keyboard hook could not be restarted.";
        }

        RefreshUi();
    }

    public void OpenSettings()
    {
        EnsureSettingsWindow().ShowSettings();
    }

    private MainWindow EnsureSettingsWindow()
    {
        if (_window is null)
        {
            _window = new MainWindow();
            MainWindow = _window;
        }

        return _window;
    }

    internal void WindowClosed(MainWindow window)
    {
        if (ReferenceEquals(_window, window))
        {
            _window = null;
        }
    }

    private void ToggleEnabled()
    {
        UpdateSettings(_settings.WithEnabled(!_settings.Enabled));
    }

    public void EmergencyDisableInput()
    {
        UpdateSettings(_settings.WithEnabled(false), persist: false);
    }

    private void OnPowerModeChanged(object? sender, PowerModeChangedEventArgs e)
    {
        if (e.Mode == PowerModes.Resume)
        {
            Dispatcher.BeginInvoke(new Action(() =>
            {
                if (!_sessionLocked)
                {
                    RecoverInputEngine("The keyboard hook was restored after Windows resumed.");
                }
            }));
        }
    }

    private void OnSessionSwitch(object? sender, SessionSwitchEventArgs e)
    {
        if (e.Reason == SessionSwitchReason.SessionLock)
        {
            Dispatcher.BeginInvoke(new Action(() =>
            {
                _sessionLocked = true;
                PauseInputEngineForLock();
            }));
        }
        else if (e.Reason == SessionSwitchReason.SessionUnlock)
        {
            Dispatcher.BeginInvoke(new Action(() =>
            {
                _sessionLocked = false;
                RecoverInputEngine("The keyboard hook was restored after Windows unlocked.");
            }));
        }
    }

    private void PauseInputEngineForLock()
    {
        if (_isQuitting)
        {
            return;
        }

        _inputEngine.Stop();
        _diagnosticMessage = "The keyboard hook is paused while Windows is locked.";
        RefreshUi();
    }

    private void RecoverInputEngine(string reason)
    {
        if (_isQuitting || _sessionLocked || !_settings.Enabled)
        {
            return;
        }

        _inputEngine.Stop();
        if (_inputEngine.Start())
        {
            _diagnosticMessage = reason;
        }
        else
        {
            _diagnosticMessage = _inputEngine.StatusError ?? "The keyboard hook could not be recovered.";
        }

        RefreshUi();
    }

    private void RefreshUi()
    {
        _trayIcon?.Update(_settings);
        _window?.ApplySettings(_settings, _recoveryMessage, DiagnosticMessage);
    }

    private void ApplyStartupRegistration(bool enabled)
    {
        var result = _startupRegistrationService.Apply(enabled);
        _startupRegistrationError = result.Succeeded ? null : result.Error;
    }

    /// <summary>
    /// Fixes the accent color to Hyperkey blue (#0B64D2) in light and dark themes
    /// so selected states match the design. High-contrast themes keep the system
    /// accent so accessibility behavior is preserved.
    /// </summary>
    private static void ApplyHyperkeyAccent()
    {
        if (ApplicationThemeManager.GetAppTheme() == ApplicationTheme.HighContrast)
        {
            return;
        }

        var accent = Color.FromRgb(0x0B, 0x64, 0xD2);
        var isDark = ApplicationThemeManager.GetAppTheme() == ApplicationTheme.Dark;
        // Accent-tinted text needs a lighter variant on dark surfaces to stay readable.
        var accentText = isDark ? Color.FromRgb(0x7B, 0xAE, 0xE8) : accent;
        var resources = Current.Resources;
        resources["SystemAccentColor"] = accent;
        resources["SystemAccentColorPrimary"] = accent;
        resources["SystemAccentColorSecondary"] = accent;
        resources["SystemAccentColorTertiary"] = accent;
        resources["SystemAccentColorPrimaryBrush"] = new SolidColorBrush(accent);
        resources["SystemAccentColorSecondaryBrush"] = new SolidColorBrush(accent);
        resources["SystemAccentColorTertiaryBrush"] = new SolidColorBrush(accent);
        resources["AccentFillColorDefaultBrush"] = new SolidColorBrush(accent);
        resources["AccentFillColorSecondaryBrush"] = new SolidColorBrush(Color.FromArgb(0xE6, accent.R, accent.G, accent.B));
        resources["AccentFillColorTertiaryBrush"] = new SolidColorBrush(Color.FromArgb(0xCC, accent.R, accent.G, accent.B));
        resources["AccentTextFillColorPrimaryBrush"] = new SolidColorBrush(accentText);
        resources["AccentTextFillColorSecondaryBrush"] = new SolidColorBrush(accentText);
        resources["AccentTextFillColorTertiaryBrush"] = new SolidColorBrush(accentText);
        resources["TextOnAccentFillColorPrimaryBrush"] = new SolidColorBrush(Colors.White);
        resources["TextOnAccentFillColorSecondaryBrush"] = new SolidColorBrush(Color.FromArgb(0xCC, 0xFF, 0xFF, 0xFF));
    }

    private void Quit()
    {
        if (_isQuitting)
        {
            return;
        }

        _isQuitting = true;
        _window?.CloseForExit();
        Shutdown();
    }

    private void OnInputEngineStatusChanged(InputEngineStatus status, string? error)
    {
        InputStatusError = error;
        if (status == InputEngineStatus.Failed)
        {
            _diagnosticMessage = error;
        }

        InputStatusChanged?.Invoke(status, error);
    }
}
