using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Runtime.CompilerServices;
using Veil.Engine;
using Veil.Engine.Native;
using Veil.Engine.Session;

namespace Veil.App;

public sealed class PanelViewModel : INotifyPropertyChanged
{
    private readonly ICcdApi _ccd;
    private readonly RecoveryCoordinator _recovery;
    private readonly Func<bool> _vddInstalled;
    private string _detail = "托盘常驻。关面板不会退出。黑色画面不是关屏成功。";
    private string _hotkeyStatus = CcdConstants.HotkeyText + "：未知";
    private bool _startupEnabled;
    private bool _busy;

    public PanelViewModel(ICcdApi ccd, RecoveryCoordinator recovery, Func<bool> vddInstalled, bool startupEnabled)
    {
        _ccd = ccd;
        _recovery = recovery;
        _vddInstalled = vddInstalled;
        _startupEnabled = startupEnabled;
    }

    public ObservableCollection<ScreenItem> Screens { get; } = [];

    public string Detail
    {
        get => _detail;
        private set => Set(ref _detail, value);
    }

    public string HotkeyStatus
    {
        get => _hotkeyStatus;
        private set => Set(ref _hotkeyStatus, value);
    }

    public bool StartupEnabled
    {
        get => _startupEnabled;
        set
        {
            if (Set(ref _startupEnabled, value))
            {
                StartupShortcut.SetEnabled(value);
            }
        }
    }

    public bool IsHolding => Screens.Any(s => s.Wanted == "保持关闭" || s.Confirmed is "处理中");

    public event PropertyChangedEventHandler? PropertyChanged;

    public void Refresh()
    {
        DisplaySnapshot snapshot;
        try
        {
            snapshot = _ccd.QuerySnapshot();
        }
        catch (Exception ex)
        {
            Detail = "无法枚举显示器：" + ex.Message;
            return;
        }

        _recovery.Poll();
        var hotkey = _recovery.HotkeyRegistered;
        HotkeyStatus = hotkey
            ? $"{CcdConstants.HotkeyText}：可用"
            : $"{CcdConstants.HotkeyText}：不可用";
        var items = ScreenListBuilder.Build(
            snapshot,
            _recovery.Heartbeat,
            _recovery.Wanted,
            _vddInstalled(),
            _recovery.IsReady || !_recovery.HasSession,
            hotkey || !_recovery.HasSession);
        Screens.Clear();
        foreach (var item in items)
        {
            Screens.Add(item);
        }

        if (_recovery.Heartbeat?.Detail is { Length: > 0 } detail)
        {
            Detail = detail;
        }

        OnPropertyChanged(nameof(IsHolding));
    }

    public string? KeepOff(ScreenItem item)
    {
        if (_busy)
        {
            return "正在处理。";
        }

        _busy = true;
        try
        {
            var error = _recovery.KeepOff(item.Identity);
            Refresh();
            if (error is not null)
            {
                Detail = error;
            }

            return error;
        }
        finally
        {
            _busy = false;
        }
    }

    public string? Restore(ScreenItem item)
    {
        var error = _recovery.RestoreOne(item.Identity);
        Refresh();
        if (error is not null)
        {
            Detail = error;
        }

        return error;
    }

    public string? RestoreAll()
    {
        var error = _recovery.RestoreAll();
        Refresh();
        if (error is not null)
        {
            Detail = error;
        }

        return error;
    }

    public bool TryExit(TimeSpan timeout, out string message)
    {
        if (!_recovery.HasSession)
        {
            message = "";
            return true;
        }

        var ok = _recovery.RestoreAllAndWait(timeout, out message);
        Refresh();
        return ok;
    }

    private bool Set<T>(ref T field, T value, [CallerMemberName] string? name = null)
    {
        if (Equals(field, value))
        {
            return false;
        }

        field = value;
        OnPropertyChanged(name);
        return true;
    }

    private void OnPropertyChanged([CallerMemberName] string? name = null) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
}
