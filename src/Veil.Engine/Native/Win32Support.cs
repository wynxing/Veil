using System.Runtime.InteropServices;
using Veil.Engine.Native;

namespace Veil.Engine.Native;

public interface IHotkey
{
    bool TryRegister();
    void Unregister();
    bool WasPressed();
}

public sealed class Win32Hotkey : IHotkey
{
    private bool _registered;

    public bool TryRegister()
    {
        _registered = NativeHotkey.RegisterHotKey(IntPtr.Zero, CcdConstants.HotkeyId, CcdConstants.HotkeyModifiers, CcdConstants.VkF10);
        return _registered;
    }

    public void Unregister()
    {
        if (_registered)
        {
            NativeHotkey.UnregisterHotKey(IntPtr.Zero, CcdConstants.HotkeyId);
            _registered = false;
        }
    }

    public bool WasPressed()
    {
        var found = false;
        while (NativeHotkey.PeekMessage(out var msg, IntPtr.Zero, CcdConstants.WmHotkey, CcdConstants.WmHotkey, 1))
        {
            found |= msg.WParam == (UIntPtr)CcdConstants.HotkeyId;
        }

        return found;
    }
}

internal static class NativeHotkey
{
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool UnregisterHotKey(IntPtr hWnd, int id);

    [DllImport("user32.dll")]
    public static extern bool PeekMessage(out NativeMessage lpMsg, IntPtr hWnd, uint wMsgFilterMin, uint wMsgFilterMax, uint wRemoveMsg);
}

[StructLayout(LayoutKind.Sequential)]
internal struct NativeMessage
{
    public IntPtr HWnd;
    public uint Message;
    public UIntPtr WParam;
    public IntPtr LParam;
    public uint Time;
    public PointL Point;
}

public interface IMonotonicClock
{
    double Seconds { get; }
}

public sealed class SystemMonotonicClock : IMonotonicClock
{
    public double Seconds => Environment.TickCount64 / 1000.0;
}

public interface IParentWatcher
{
    bool IsAlive(int pid);
}

public sealed class Win32ParentWatcher : IParentWatcher
{
    private const uint ProcessQueryLimitedInformation = 0x1000;
    private const uint StillActive = 259;

    public bool IsAlive(int pid)
    {
        if (pid <= 0)
        {
            return false;
        }

        var handle = NativeProcess.OpenProcess(ProcessQueryLimitedInformation, false, (uint)pid);
        if (handle == IntPtr.Zero)
        {
            return false;
        }

        try
        {
            if (!NativeProcess.GetExitCodeProcess(handle, out var code))
            {
                return false;
            }

            return code == StillActive;
        }
        finally
        {
            NativeProcess.CloseHandle(handle);
        }
    }
}

internal static class NativeProcess
{
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr OpenProcess(uint dwDesiredAccess, bool bInheritHandle, uint dwProcessId);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool GetExitCodeProcess(IntPtr hProcess, out uint lpExitCode);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool CloseHandle(IntPtr hObject);
}
