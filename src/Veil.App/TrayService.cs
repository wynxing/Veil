using System.Drawing;
using System.Windows.Forms;

namespace Veil.App;

internal sealed class TrayService : IDisposable
{
    private readonly NotifyIcon _icon;
    private readonly Action _open;
    private readonly Action _restore;
    private readonly Action _exit;

    public TrayService(Action open, Action restore, Action exit)
    {
        _open = open;
        _restore = restore;
        _exit = exit;
        _icon = new NotifyIcon
        {
            Text = "Veil",
            Icon = SystemIcons.Application,
            Visible = true,
        };
        _icon.MouseClick += (_, e) =>
        {
            if (e.Button == MouseButtons.Left)
            {
                _open();
            }
        };
        var menu = new ContextMenuStrip();
        menu.Items.Add("打开面板", null, (_, _) => _open());
        menu.Items.Add("恢复全部", null, (_, _) => _restore());
        menu.Items.Add("退出", null, (_, _) => _exit());
        _icon.ContextMenuStrip = menu;
    }

    public void SetHolding(bool holding)
    {
        _icon.Text = holding ? "Veil（保持关闭中）" : "Veil";
        _icon.Icon = holding ? SystemIcons.Information : SystemIcons.Application;
    }

    public void Dispose()
    {
        _icon.Visible = false;
        _icon.Dispose();
    }
}
