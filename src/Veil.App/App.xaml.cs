using System.Windows;
using System.Windows.Threading;
using Veil.Engine.Native;
using Application = System.Windows.Application;
using MessageBox = System.Windows.MessageBox;

namespace Veil.App;

public partial class App : Application
{
    private nint _mutex;
    private TrayService? _tray;
    private MainWindow? _window;
    private PanelViewModel? _vm;
    private DispatcherTimer? _timer;

    protected override void OnStartup(StartupEventArgs e)
    {
        if (e.Args.Any(a => a is "--restore-and-exit"))
        {
            OpenSessionRelease.RequestAll();
            Shutdown();
            return;
        }

        if (!SingleInstance.TryAcquire(out _mutex))
        {
            Shutdown();
            return;
        }

        base.OnStartup(e);
        var ccd = new Win32CcdApi();
        var recovery = new RecoveryCoordinator(ccd);
        _vm = new PanelViewModel(ccd, recovery, () => DriverStatus.Installed, StartupShortcut.IsEnabled());
        _window = new MainWindow(_vm);
        _tray = new TrayService(
            ShowPanel,
            () => _vm.RestoreAll(),
            ExitApp);
        _timer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(800) };
        _timer.Tick += (_, _) =>
        {
            _vm.Refresh();
            _tray.SetHolding(_vm.IsHolding);
        };
        _timer.Start();
        _vm.Refresh();
        _window.Show();
    }

    private void ShowPanel()
    {
        if (_window is null)
        {
            return;
        }

        _window.Show();
        _window.Activate();
        _window.WindowState = WindowState.Normal;
    }

    private void ExitApp()
    {
        if (_vm is null)
        {
            Shutdown();
            return;
        }

        if (!_vm.TryExit(TimeSpan.FromSeconds(20), out var message))
        {
            MessageBox.Show(message, "Veil", MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }

        Shutdown();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _timer?.Stop();
        _tray?.Dispose();
        SingleInstance.Release(_mutex);
        base.OnExit(e);
    }
}
