using System.Runtime.InteropServices;

namespace Veil.App;

internal static class SingleInstance
{
    public const string MutexName = @"Local\Veil";
    private const int ErrorAlreadyExists = 183;

    public static bool TryAcquire(out nint handle)
    {
        Native.SetLastError(0);
        handle = Native.CreateMutex(IntPtr.Zero, true, MutexName);
        if (handle == IntPtr.Zero)
        {
            return false;
        }

        return Native.GetLastError() != ErrorAlreadyExists;
    }

    public static void Release(nint handle)
    {
        if (handle != IntPtr.Zero)
        {
            Native.CloseHandle(handle);
        }
    }

    private static class Native
    {
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern nint CreateMutex(IntPtr lpMutexAttributes, bool bInitialOwner, string lpName);

        [DllImport("kernel32.dll")]
        public static extern uint GetLastError();

        [DllImport("kernel32.dll")]
        public static extern bool CloseHandle(nint hObject);

        [DllImport("kernel32.dll")]
        public static extern void SetLastError(uint dwErrCode);
    }
}
