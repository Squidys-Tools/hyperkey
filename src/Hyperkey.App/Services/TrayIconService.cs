using System.Drawing;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Hyperkey.Core;
using Wpf.Ui.Tray.Controls;
using UiContextMenu = System.Windows.Controls.ContextMenu;
using UiMenuItem = Wpf.Ui.Controls.MenuItem;
using UiSeparator = System.Windows.Controls.Separator;

namespace Hyperkey.App.Services;

public sealed class TrayIconService : IDisposable
{
    private readonly NotifyIcon _notifyIcon;
    private readonly Action _toggleEnabled;
    private readonly Action _openSettings;
    private readonly UiMenuItem _toggleItem;
    private bool _disposed;

    public TrayIconService(
        HyperkeySettings settings,
        Action toggleEnabled,
        Action openSettings,
        Action quit)
    {
        _toggleEnabled = toggleEnabled;
        _openSettings = openSettings;

        _toggleItem = new UiMenuItem
        {
            IsCheckable = true
        };
        _toggleItem.Click += ToggleItem_Click;

        var openSettingsItem = new UiMenuItem { Header = "Open settings" };
        openSettingsItem.Click += (_, _) => openSettings();

        var quitItem = new UiMenuItem { Header = "Quit" };
        quitItem.Click += (_, _) => quit();

        var menu = new UiContextMenu();
        menu.Items.Add(_toggleItem);
        menu.Items.Add(new UiSeparator());
        menu.Items.Add(openSettingsItem);
        menu.Items.Add(quitItem);

        _notifyIcon = new NotifyIcon
        {
            FocusOnLeftClick = true,
            Icon = CreateApplicationIcon(),
            Menu = menu,
            MenuOnRightClick = true,
            TooltipText = "Hyperkey"
        };
#pragma warning disable CS8622 // WPF-UI.Tray's event delegate has inconsistent nullable metadata.
        _notifyIcon.LeftDoubleClick += OnLeftDoubleClick;
#pragma warning restore CS8622
        _notifyIcon.Register();

        Update(settings);
    }

    public void Update(HyperkeySettings settings)
    {
        _toggleItem.Header = settings.Enabled ? "Hyperkey: On" : "Hyperkey: Off";
        _toggleItem.IsChecked = settings.Enabled;
        _notifyIcon.TooltipText = settings.Enabled ? "Hyperkey: On" : "Hyperkey: Off";
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        _notifyIcon.Dispose();
    }

    private void ToggleItem_Click(object sender, RoutedEventArgs e) => _toggleEnabled();

    private void OnLeftDoubleClick(NotifyIcon sender, RoutedEventArgs e) => _openSettings();

    public bool IsRegistered => _notifyIcon.IsRegistered;

    private static ImageSource CreateApplicationIcon()
    {
        var source = Imaging.CreateBitmapSourceFromHIcon(
            SystemIcons.Application.Handle,
            Int32Rect.Empty,
            BitmapSizeOptions.FromEmptyOptions());
        source.Freeze();
        return source;
    }
}
