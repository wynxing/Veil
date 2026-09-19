using System.Runtime.InteropServices;
using System.Text;
using Veil.Engine.Native;

namespace Veil.Engine.Runtime;

public static class ProcessLaunch
{
    public static int StartDetached(string fileName, string arguments)
    {
        var command = $"\"{fileName}\" {arguments}";
        var si = new StartupInfo { Cb = Marshal.SizeOf<StartupInfo>() };
        var created = NativeCreateProcess.CreateProcess(
            null,
            new StringBuilder(command),
            IntPtr.Zero,
            IntPtr.Zero,
            false,
            CcdConstants.CreateBreakawayFromJob | CcdConstants.CreateNewProcessGroup | CcdConstants.CreateNoWindow,
            IntPtr.Zero,
            null,
            ref si,
            out var pi);
        if (!created)
        {
            created = NativeCreateProcess.CreateProcess(
                null,
                new StringBuilder(command),
                IntPtr.Zero,
                IntPtr.Zero,
                false,
                CcdConstants.CreateNewProcessGroup | CcdConstants.CreateNoWindow,
                IntPtr.Zero,
                null,
                ref si,
                out pi);
        }

        if (!created)
        {
            throw new InvalidOperationException($"CreateProcess failed: {Marshal.GetLastWin32Error()}");
        }

        NativeCreateProcess.CloseHandle(pi.HThread);
        NativeCreateProcess.CloseHandle(pi.HProcess);
        return (int)pi.DwProcessId;
    }

    public static string RecoveryExePath()
    {
        var dir = AppContext.BaseDirectory;
        var candidate = Path.Combine(dir, "Veil.Recovery.exe");
        if (File.Exists(candidate))
        {
            return candidate;
        }

        candidate = Path.Combine(dir, "..", "Veil.Recovery", "Veil.Recovery.exe");
        return Path.GetFullPath(candidate);
    }

    public static string DriverHelperExePath()
    {
        var dir = AppContext.BaseDirectory;
        var candidate = Path.Combine(dir, "Veil.DriverHelper.exe");
        if (File.Exists(candidate))
        {
            return candidate;
        }

        return Path.GetFullPath(Path.Combine(dir, "..", "Veil.DriverHelper", "Veil.DriverHelper.exe"));
    }
}

[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
internal struct StartupInfo
{
    public int Cb;
    public string? LpReserved;
    public string? LpDesktop;
    public string? LpTitle;
    public int DwX;
    public int DwY;
    public int DwXSize;
    public int DwYSize;
    public int DwXCountChars;
    public int DwYCountChars;
    public int DwFillAttribute;
    public int DwFlags;
    public short WShowWindow;
    public short CbReserved2;
    public IntPtr LpReserved2;
    public IntPtr HStdInput;
    public IntPtr HStdOutput;
    public IntPtr HStdError;
}

[StructLayout(LayoutKind.Sequential)]
internal struct ProcessInformation
{
    public IntPtr HProcess;
    public IntPtr HThread;
    public uint DwProcessId;
    public uint DwThreadId;
}

internal static class NativeCreateProcess
{
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern bool CreateProcess(
        string? lpApplicationName,
        StringBuilder lpCommandLine,
        IntPtr lpProcessAttributes,
        IntPtr lpThreadAttributes,
        bool bInheritHandles,
        int dwCreationFlags,
        IntPtr lpEnvironment,
        string? lpCurrentDirectory,
        ref StartupInfo lpStartupInfo,
        out ProcessInformation lpProcessInformation);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool CloseHandle(IntPtr hObject);
}
