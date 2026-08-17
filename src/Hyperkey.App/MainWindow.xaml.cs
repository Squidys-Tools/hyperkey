using System.Collections.Immutable;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using Hyperkey.Core;
using Hyperkey.Input;
using FluentWindow = Wpf.Ui.Controls.FluentWindow;

namespace Hyperkey.App;

public partial class MainWindow : FluentWindow
{
    private static readonly IReadOnlyList<TriggerOption> TriggerOptions =
    [
        new(TriggerKey.CapsLock, "Caps Lock"),
        new(TriggerKey.ScrollLock, "Scroll Lock")
    ];

    private bool _isApplyingSettings;
    private bool _allowClose;

    public MainWindow()
    {
        InitializeComponent();
        VersionText.Text = $"Version {AppVersion.Display}";
        TriggerKeySelector.ItemsSource = TriggerOptions;
        TriggerKeySelector.DisplayMemberPath = nameof(TriggerOption.Label);

        Closing += MainWindow_Closing;
        App.CurrentApp.SettingsChanged += OnSettingsChanged;
        App.CurrentApp.InputStatusChanged += OnInputStatusChanged;
        ApplySettings(
            App.CurrentApp.Settings,
            App.CurrentApp.RecoveryMessage,
            App.CurrentApp.DiagnosticMessage);
    }

    public void ShowSettings()
    {
        if (WindowState == WindowState.Minimized)
        {
            WindowState = WindowState.Normal;
        }

        Show();
        Activate();
    }

    public void PrepareForTray()
    {
        var originalShowInTaskbar = ShowInTaskbar;
        var originalOpacity = Opacity;
        var originalShowActivated = ShowActivated;

        ShowInTaskbar = false;
        Opacity = 0;
        ShowActivated = false;
        Show();
        Hide();

        ShowInTaskbar = originalShowInTaskbar;
        Opacity = originalOpacity;
        ShowActivated = originalShowActivated;
    }

    public void CloseForExit()
    {
        _allowClose = true;
        Close();
    }

    public void ApplySettings(
        HyperkeySettings settings,
        string? recoveryMessage,
        string? diagnosticMessage)
    {
        _isApplyingSettings = true;
        try
        {
            EnabledToggle.IsChecked = settings.Enabled;
            LaunchToggle.IsChecked = settings.LaunchAtStartup;
            LaunchToTrayToggle.IsChecked = settings.LaunchToTray;
            TriggerKeySelector.SelectedItem = TriggerOptions.Single(option => option.Key == settings.Trigger);
            ControlModifierCheckBox.IsChecked = settings.OutputModifiers.Contains(OutputModifier.Control);
            AltModifierCheckBox.IsChecked = settings.OutputModifiers.Contains(OutputModifier.Alt);
            ShiftModifierCheckBox.IsChecked = settings.OutputModifiers.Contains(OutputModifier.Shift);

            StatusText.Text = settings.Enabled ? "Hyperkey is on" : "Hyperkey is off";
            StatusDescription.Text = GetStatusDescription(settings);

            ModifierSelectionHintText.Visibility = Visibility.Visible;

            RecoveryText.Text = recoveryMessage is null
                ? string.Empty
                : $"Hyperkey loaded safe defaults. {recoveryMessage}";
            RecoveryBanner.Visibility = recoveryMessage is null
                ? Visibility.Collapsed
                : Visibility.Visible;

            DiagnosticStatusText.Text = GetInputStatusLabel(App.CurrentApp.InputStatus);
            DiagnosticMessageText.Text = diagnosticMessage ?? string.Empty;
            DiagnosticMessageText.Visibility = diagnosticMessage is null
                ? Visibility.Collapsed
                : Visibility.Visible;
            DiagnosticCopyStatusText.Visibility = Visibility.Collapsed;
        }
        finally
        {
            _isApplyingSettings = false;
        }
    }

    private void EnabledToggle_Click(object sender, RoutedEventArgs e)
    {
        if (!_isApplyingSettings)
        {
            App.CurrentApp.UpdateSettings(App.CurrentApp.Settings.WithEnabled(EnabledToggle.IsChecked == true));
        }
    }

    private void LaunchToggle_Click(object sender, RoutedEventArgs e)
    {
        if (!_isApplyingSettings)
        {
            App.CurrentApp.UpdateSettings(App.CurrentApp.Settings.WithLaunchAtStartup(LaunchToggle.IsChecked == true));
        }
    }

    private void LaunchToTrayToggle_Click(object sender, RoutedEventArgs e)
    {
        if (!_isApplyingSettings)
        {
            App.CurrentApp.UpdateSettings(
                App.CurrentApp.Settings.WithLaunchToTray(LaunchToTrayToggle.IsChecked == true));
        }
    }

    private void TriggerKeySelector_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (!_isApplyingSettings && TriggerKeySelector.SelectedItem is TriggerOption option)
        {
            App.CurrentApp.UpdateSettings(App.CurrentApp.Settings.WithTrigger(option.Key));
        }
    }

    private void OutputModifierCheckBox_Click(object sender, RoutedEventArgs e)
    {
        if (_isApplyingSettings)
        {
            return;
        }

        var modifiers = GetSelectedOutputModifiers();
        if (modifiers.IsDefaultOrEmpty)
        {
            if (sender is CheckBox checkBox)
            {
                checkBox.IsChecked = true;
            }

            ModifierSelectionHintText.Text = "Choose at least one modifier.";
            ModifierSelectionHintText.Visibility = Visibility.Visible;
            return;
        }

        ModifierSelectionHintText.Text = "Choose one or more modifiers.";
        ModifierSelectionHintText.Visibility = Visibility.Visible;
        App.CurrentApp.UpdateSettings(App.CurrentApp.Settings.WithOutputModifiers(modifiers));
    }

    private void RestartHook_Click(object sender, RoutedEventArgs e)
    {
        App.CurrentApp.RestartInputEngine();
    }

    private void EmergencyDisable_Click(object sender, RoutedEventArgs e)
    {
        App.CurrentApp.EmergencyDisableInput();
    }

    private void CopyDiagnostics_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            Clipboard.SetText(App.CurrentApp.DiagnosticDetails);
            DiagnosticCopyStatusText.Text = "Diagnostic details copied to the clipboard.";
        }
        catch (Exception exception) when (exception is ExternalException
            or ArgumentException
            or InvalidOperationException)
        {
            DiagnosticCopyStatusText.Text = $"Diagnostic details could not be copied: {exception.Message}";
        }

        DiagnosticCopyStatusText.Visibility = Visibility.Visible;
    }

    private void OnSettingsChanged(HyperkeySettings settings)
    {
        ApplySettings(settings, App.CurrentApp.RecoveryMessage, App.CurrentApp.DiagnosticMessage);
    }

    private void OnInputStatusChanged(InputEngineStatus status, string? error)
    {
        if (Dispatcher.CheckAccess())
        {
            ApplySettings(
                App.CurrentApp.Settings,
                App.CurrentApp.RecoveryMessage,
                App.CurrentApp.DiagnosticMessage);
            return;
        }

        Dispatcher.BeginInvoke(
            new Action(() => ApplySettings(
                App.CurrentApp.Settings,
                App.CurrentApp.RecoveryMessage,
                App.CurrentApp.DiagnosticMessage)));
    }

    private void MainWindow_Closing(object? sender, CancelEventArgs args)
    {
        if (_allowClose)
        {
            App.CurrentApp.WindowClosed(this);
            App.CurrentApp.SettingsChanged -= OnSettingsChanged;
            App.CurrentApp.InputStatusChanged -= OnInputStatusChanged;
            return;
        }

        args.Cancel = true;
        Hide();
    }

    private ImmutableArray<OutputModifier> GetSelectedOutputModifiers()
    {
        var selected = ImmutableArray.CreateBuilder<OutputModifier>(3);
        if (ControlModifierCheckBox.IsChecked == true)
        {
            selected.Add(OutputModifier.Control);
        }

        if (AltModifierCheckBox.IsChecked == true)
        {
            selected.Add(OutputModifier.Alt);
        }

        if (ShiftModifierCheckBox.IsChecked == true)
        {
            selected.Add(OutputModifier.Shift);
        }

        return selected.ToImmutable();
    }

    private static string GetModifierLabel(OutputModifier modifier) => modifier switch
    {
        OutputModifier.Control => "Ctrl",
        OutputModifier.Alt => "Alt",
        OutputModifier.Shift => "Shift",
        _ => throw new ArgumentOutOfRangeException(nameof(modifier))
    };

    private static string GetTriggerLabel(TriggerKey trigger) => trigger switch
    {
        TriggerKey.CapsLock => "Caps Lock",
        TriggerKey.ScrollLock => "Scroll Lock",
        _ => throw new ArgumentOutOfRangeException(nameof(trigger))
    };

    private static string GetStatusDescription(HyperkeySettings settings)
    {
        if (!settings.Enabled)
        {
            return "The modifier layer is currently disabled.";
        }

        return App.CurrentApp.InputStatus switch
        {
            InputEngineStatus.Starting => "Starting the keyboard hook...",
            InputEngineStatus.Running => $"Hold {GetTriggerLabel(settings.Trigger)} to use {string.Join(" + ", settings.OutputModifiers.Select(GetModifierLabel))}.",
            InputEngineStatus.Stopping => "Stopping the keyboard hook...",
            InputEngineStatus.Failed => $"The keyboard hook is unavailable. {App.CurrentApp.InputStatusError ?? "No error details were reported."}",
            _ => "The keyboard engine is stopped."
        };
    }

    private static string GetInputStatusLabel(InputEngineStatus status) => status switch
    {
        InputEngineStatus.Starting => "Starting",
        InputEngineStatus.Running => "Running",
        InputEngineStatus.Stopping => "Stopping",
        InputEngineStatus.Failed => "Needs attention",
        _ => "Stopped"
    };

    private sealed record TriggerOption(TriggerKey Key, string Label);
}
