using System.Runtime.InteropServices;
using System.Text;
using Veil.Engine;

namespace Veil.Engine.Native;

public sealed record CcdFrame(
    DisplayConfigPathInfo[] Paths,
    DisplayConfigModeInfo[] Modes,
    DisplaySnapshot Snapshot);

public interface ICcdApi
{
    (DisplayConfigPathInfo[] Paths, DisplayConfigModeInfo[] Modes) QueryRaw(uint flags = CcdConstants.QueryFlags);
    CcdFrame Capture(uint flags = CcdConstants.QueryFlags);
    int Set(DisplayConfigPathInfo[] paths, DisplayConfigModeInfo[] modes, uint flags);
    int SetTopology(uint topologyFlags);
    DisplaySnapshot QuerySnapshot(uint flags = CcdConstants.QueryFlags);
    string? GetLastErrorMessage(int code);
}

public sealed class Win32CcdApi : ICcdApi
{
    public (DisplayConfigPathInfo[] Paths, DisplayConfigModeInfo[] Modes) QueryRaw(uint flags = CcdConstants.QueryFlags)
    {
        for (var attempt = 0; attempt < 8; attempt++)
        {
            var rc = NativeMethods.GetDisplayConfigBufferSizes(flags, out var pathCount, out var modeCount);
            if (rc != CcdConstants.ErrorSuccess)
            {
                throw new InvalidOperationException($"GetDisplayConfigBufferSizes failed: {Win32Message(rc)}");
            }

            var paths = new DisplayConfigPathInfo[pathCount];
            var modes = new DisplayConfigModeInfo[modeCount];
            rc = NativeMethods.QueryDisplayConfig(flags, ref pathCount, paths, ref modeCount, modes, IntPtr.Zero);
            if (rc == CcdConstants.ErrorInsufficientBuffer)
            {
                continue;
            }

            if (rc != CcdConstants.ErrorSuccess)
            {
                throw new InvalidOperationException($"QueryDisplayConfig failed: {Win32Message(rc)}");
            }

            if (pathCount != paths.Length)
            {
                Array.Resize(ref paths, (int)pathCount);
            }

            if (modeCount != modes.Length)
            {
                Array.Resize(ref modes, (int)modeCount);
            }

            return (paths, modes);
        }

        throw new InvalidOperationException("QueryDisplayConfig buffer retry exhausted");
    }

    public int Set(DisplayConfigPathInfo[] paths, DisplayConfigModeInfo[] modes, uint flags)
    {
        if ((flags & CcdConstants.SdcSaveToDatabase) != 0)
        {
            throw new InvalidOperationException("SDC_SAVE_TO_DATABASE is forbidden");
        }

        return NativeMethods.SetDisplayConfig(
            (uint)paths.Length,
            paths.Length == 0 ? null : paths,
            (uint)modes.Length,
            modes.Length == 0 ? null : modes,
            flags);
    }

    public int SetTopology(uint topologyFlags)
    {
        if ((topologyFlags & CcdConstants.SdcSaveToDatabase) != 0)
        {
            throw new InvalidOperationException("SDC_SAVE_TO_DATABASE is forbidden");
        }

        return NativeMethods.SetDisplayConfig(0, null, 0, null, topologyFlags);
    }

    public CcdFrame Capture(uint flags = CcdConstants.QueryFlags)
    {
        var (paths, modes) = QueryRaw(flags);
        var rows = new List<PathRow>(paths.Length);
        for (var i = 0; i < paths.Length; i++)
        {
            rows.Add(Describe(paths[i], i));
        }

        return new CcdFrame(paths, modes, new DisplaySnapshot(rows, NativeMethods.GetSystemMetrics(CcdConstants.SmCmonitors)));
    }

    public DisplaySnapshot QuerySnapshot(uint flags = CcdConstants.QueryFlags) => Capture(flags).Snapshot;

    public string? GetLastErrorMessage(int code) => Win32Message(code);

    public static PathRow Describe(DisplayConfigPathInfo path, int index)
    {
        var target = TargetName(path);
        var adapterPath = AdapterName(path);
        var sourceName = SourceName(path);
        var monitorPath = target.Path ?? "";
        var placeholder = monitorPath.Contains("DEFAULT_MONITOR", StringComparison.OrdinalIgnoreCase);
        var internalTech = Roles.IsInternalTechnology(path.TargetInfo.OutputTechnology);
        var role = Roles.Classify(placeholder, internalTech, adapterPath, monitorPath, target.Name ?? "", sourceName);
        return new PathRow(
            index,
            (path.Flags & CcdConstants.DisplayConfigPathActive) != 0,
            path.Flags,
            path.SourceInfo.Id,
            path.TargetInfo.Id,
            path.TargetInfo.AdapterId.ToHex(),
            path.TargetInfo.OutputTechnology,
            internalTech,
            sourceName,
            adapterPath,
            target.Name ?? "",
            monitorPath,
            placeholder,
            role,
            target.EdidManufactureId,
            target.EdidProductCodeId);
    }

    private static (string? Name, string? Path, ushort EdidManufactureId, ushort EdidProductCodeId) TargetName(DisplayConfigPathInfo path)
    {
        var info = new DisplayConfigTargetDeviceName
        {
            Header = new DisplayConfigDeviceInfoHeader
            {
                Type = CcdConstants.DisplayConfigDeviceInfoGetTargetName,
                Size = (uint)Marshal.SizeOf<DisplayConfigTargetDeviceName>(),
                AdapterId = path.TargetInfo.AdapterId,
                Id = path.TargetInfo.Id,
            },
            MonitorFriendlyDeviceName = "",
            MonitorDevicePath = "",
        };
        var rc = NativeMethods.DisplayConfigGetDeviceInfo(ref info);
        if (rc != 0)
        {
            return ("", "", 0, 0);
        }

        return (info.MonitorFriendlyDeviceName, info.MonitorDevicePath, info.EdidManufactureId, info.EdidProductCodeId);
    }

    private static string AdapterName(DisplayConfigPathInfo path)
    {
        var info = new DisplayConfigAdapterName
        {
            Header = new DisplayConfigDeviceInfoHeader
            {
                Type = CcdConstants.DisplayConfigDeviceInfoGetAdapterName,
                Size = (uint)Marshal.SizeOf<DisplayConfigAdapterName>(),
                AdapterId = path.TargetInfo.AdapterId,
                Id = path.TargetInfo.Id,
            },
            AdapterDevicePath = "",
        };
        var rc = NativeMethods.DisplayConfigGetDeviceInfoAdapter(ref info);
        return rc == 0 ? info.AdapterDevicePath ?? "" : "";
    }

    private static string SourceName(DisplayConfigPathInfo path)
    {
        var info = new DisplayConfigSourceDeviceName
        {
            Header = new DisplayConfigDeviceInfoHeader
            {
                Type = CcdConstants.DisplayConfigDeviceInfoGetSourceName,
                Size = (uint)Marshal.SizeOf<DisplayConfigSourceDeviceName>(),
                AdapterId = path.SourceInfo.AdapterId,
                Id = path.SourceInfo.Id,
            },
            ViewGdiDeviceName = "",
        };
        var rc = NativeMethods.DisplayConfigGetDeviceInfoSource(ref info);
        return rc == 0 ? info.ViewGdiDeviceName ?? "" : "";
    }

    private static string Win32Message(int code)
    {
        var buffer = new StringBuilder(1024);
        var n = NativeMethods.FormatMessage(0x00001000 | 0x00000200, IntPtr.Zero, (uint)code, 0, buffer, 1024, IntPtr.Zero);
        var text = n > 0 ? buffer.ToString().Trim() : "";
        return string.IsNullOrEmpty(text) ? code.ToString() : $"{code} {text}";
    }
}

internal static class NativeMethods
{
    [DllImport("user32.dll")]
    public static extern int GetDisplayConfigBufferSizes(uint flags, out uint numPathArrayElements, out uint numModeInfoArrayElements);

    [DllImport("user32.dll")]
    public static extern int QueryDisplayConfig(
        uint flags,
        ref uint numPathArrayElements,
        [Out] DisplayConfigPathInfo[] pathArray,
        ref uint numModeInfoArrayElements,
        [Out] DisplayConfigModeInfo[] modeInfoArray,
        IntPtr currentTopologyId);

    [DllImport("user32.dll")]
    public static extern int SetDisplayConfig(
        uint numPathArrayElements,
        [In] DisplayConfigPathInfo[]? pathArray,
        uint numModeInfoArrayElements,
        [In] DisplayConfigModeInfo[]? modeInfoArray,
        uint flags);

    [DllImport("user32.dll", EntryPoint = "DisplayConfigGetDeviceInfo")]
    public static extern int DisplayConfigGetDeviceInfo(ref DisplayConfigTargetDeviceName deviceName);

    [DllImport("user32.dll", EntryPoint = "DisplayConfigGetDeviceInfo")]
    public static extern int DisplayConfigGetDeviceInfoAdapter(ref DisplayConfigAdapterName deviceName);

    [DllImport("user32.dll", EntryPoint = "DisplayConfigGetDeviceInfo")]
    public static extern int DisplayConfigGetDeviceInfoSource(ref DisplayConfigSourceDeviceName deviceName);

    [DllImport("user32.dll")]
    public static extern int GetSystemMetrics(int nIndex);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    public static extern uint FormatMessage(uint dwFlags, IntPtr lpSource, uint dwMessageId, uint dwLanguageId, StringBuilder lpBuffer, int nSize, IntPtr arguments);
}
