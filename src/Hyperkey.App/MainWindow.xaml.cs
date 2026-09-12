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
    private bool _isApplyingSettings;
    private bool _isCapturingTrigger;
    private bool _allowClose;

    public MainWindow()
    {
        InitializeComponent();
        VersionText.Text = $"Version {AppVersion.Display}";
        PreviewKeyDown += MainWindow_PreviewKeyDown;

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
            ExitTriggerCapture();
            TriggerBindButton.Content = GetTriggerLabel(settings.Trigger);
            ControlModifierCheckBox.IsChecked = settings.OutputModifiers.Contains(OutputModifier.Control);
            AltModifierCheckBox.IsChecked = settings.OutputModifiers.Contains(OutputModifier.Alt);
            ShiftModifierCheckBox.IsChecked = settings.OutputModifiers.Contains(OutputModifier.Shift);

            UpdateDiagnosticStatus(App.CurrentApp.InputStatus);

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

    private void TriggerBindButton_Click(object sender, RoutedEventArgs e)
    {
        if (_isApplyingSettings || _isCapturingTrigger)
        {
            return;
        }

        EnterTriggerCapture();
    }

    private void MainWindow_PreviewKeyDown(object sender, System.Windows.Input.KeyEventArgs e)
    {
        if (!_isCapturingTrigger)
        {
            return;
        }

        e.Handled = true;
        if (e.Key == System.Windows.Input.Key.Escape)
        {
            ExitTriggerCapture();
            return;
        }

        var trigger = MapKeyToTrigger(e.Key);
        if (trigger.HasValue)
        {
            App.CurrentApp.UpdateSettings(App.CurrentApp.Settings.WithTrigger(trigger.Value));
        }
        else
        {
            TriggerBindButton.Content = "Press a key…";
            TriggerHintText.Text = "Only Caps Lock or Scroll Lock can be the trigger. Try again, or press Esc to cancel.";
        }
    }

    private static TriggerKey? MapKeyToTrigger(System.Windows.Input.Key key) => key switch
    {
        System.Windows.Input.Key.CapsLock => TriggerKey.CapsLock,
        System.Windows.Input.Key.Scroll => TriggerKey.ScrollLock,
        _ => null
    };

    private void EnterTriggerCapture()
    {
        _isCapturingTrigger = true;
        TriggerBindButton.Content = "Press a key…";
        TriggerHintText.Text = "Press Caps Lock or Scroll Lock. Esc cancels.";
    }

    private void ExitTriggerCapture()
    {
        if (!_isCapturingTrigger)
        {
            TriggerHintText.Text = "Select to rebind — then press Caps Lock or Scroll Lock.";
            return;
        }

        _isCapturingTrigger = false;
        TriggerBindButton.Content = GetTriggerLabel(App.CurrentApp.Settings.Trigger);
        TriggerHintText.Text = "Select to rebind — then press Caps Lock or Scroll Lock.";
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

    private static string GetTriggerLabel(TriggerKey trigger) => trigger switch
    {
        TriggerKey.CapsLock => "Caps Lock",
        TriggerKey.ScrollLock => "Scroll Lock",
        _ => throw new ArgumentOutOfRangeException(nameof(trigger))
    };

    private void UpdateDiagnosticStatus(InputEngineStatus status)
    {
        DiagnosticStatusText.Text = GetInputStatusLabel(status);
    }

    private static string GetInputStatusLabel(InputEngineStatus status) => status switch
    {
        InputEngineStatus.Starting => "Starting",
        InputEngineStatus.Running => "Running",
        InputEngineStatus.Stopping => "Stopping",
        InputEngineStatus.Failed => "Needs attention",
        _ => "Stopped"
    };
}
