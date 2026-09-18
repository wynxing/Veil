using System.Runtime.InteropServices;

namespace Veil.Engine.Native;

public static class CcdConstants
{
    public const int ErrorSuccess = 0;
    public const int ErrorInsufficientBuffer = 122;

    public const uint QdcOnlyActivePaths = 0x00000002;
    public const uint QdcVirtualModeAware = 0x00000010;
    public const uint QdcVirtualRefreshRateAware = 0x00000040;

    public const uint SdcTopologyInternal = 0x00000001;
    public const uint SdcTopologyClone = 0x00000002;
    public const uint SdcUseSuppliedDisplayConfig = 0x00000020;
    public const uint SdcValidate = 0x00000040;
    public const uint SdcApply = 0x00000080;
    public const uint SdcSaveToDatabase = 0x00000200;
    public const uint SdcAllowChanges = 0x00000400;
    public const uint SdcVirtualModeAware = 0x00008000;
    public const uint SdcVirtualRefreshRateAware = 0x00020000;

    public const uint DisplayConfigPathActive = 0x00000001;
    public const uint DisplayConfigModeInfoTypeSource = 1;
    public const uint DisplayConfigModeInfoTypeTarget = 2;
    public const uint DisplayConfigPathSourceModeIdxInvalid = 0xFFFF;

    public const uint DisplayConfigDeviceInfoGetSourceName = 1;
    public const uint DisplayConfigDeviceInfoGetTargetName = 2;
    public const uint DisplayConfigDeviceInfoGetAdapterName = 4;

    public const uint OutputTechnologyInternal = 0x80000000;
    public const uint OutputTechnologyDisplayPortEmbedded = 11;
    public const uint OutputTechnologyUdiEmbedded = 13;

    public const int SmCmonitors = 80;

    public const uint QueryFlags =
        QdcOnlyActivePaths | QdcVirtualModeAware | QdcVirtualRefreshRateAware;

    public const uint SetBaseFlags =
        SdcUseSuppliedDisplayConfig
        | SdcAllowChanges
        | SdcVirtualModeAware
        | SdcVirtualRefreshRateAware;

    public const uint ValidateFlags = SetBaseFlags | SdcValidate;
    public const uint ApplyFlags = SetBaseFlags | SdcApply;

    public const int CreateNewProcessGroup = 0x00000200;
    public const int CreateBreakawayFromJob = 0x01000000;
    public const int CreateNoWindow = 0x08000000;

    public const uint ModNorepeat = 0x4000;
    public const uint ModShift = 0x0004;
    public const uint ModControl = 0x0002;
    public const uint ModAlt = 0x0001;
    public const uint HotkeyModifiers = ModNorepeat | ModShift | ModControl | ModAlt; // 0x4007
    public const uint VkF10 = 0x79;
    public const int WmHotkey = 0x0312;
    public const int HotkeyId = 1;

    public const string HotkeyText = "Ctrl+Alt+Shift+F10";
    public const string BundledHardwareId = @"Root\MttVDD";
}

[StructLayout(LayoutKind.Sequential)]
public struct Luid
{
    public uint LowPart;
    public int HighPart;

    public string ToHex() => $"{HighPart:x8}{LowPart:x8}";
}

[StructLayout(LayoutKind.Sequential)]
public struct DisplayConfigRational
{
    public uint Numerator;
    public uint Denominator;
}

[StructLayout(LayoutKind.Sequential)]
public struct DisplayConfig2DRegion
{
    public uint Cx;
    public uint Cy;
}

[StructLayout(LayoutKind.Sequential)]
public struct DisplayConfigPathSourceInfo
{
    public Luid AdapterId;
    public uint Id;
    public uint ModeInfoIdx;
    public uint StatusFlags;
}

[StructLayout(LayoutKind.Sequential)]
public struct DisplayConfigPathTargetInfo
{
    public Luid AdapterId;
    public uint Id;
    public uint ModeInfoIdx;
    public uint OutputTechnology;
    public uint Rotation;
    public uint Scaling;
    public DisplayConfigRational RefreshRate;
    public uint ScanLineOrdering;
    public int TargetAvailable;
    public uint StatusFlags;
}

[StructLayout(LayoutKind.Sequential)]
public struct DisplayConfigPathInfo
{
    public DisplayConfigPathSourceInfo SourceInfo;
    public DisplayConfigPathTargetInfo TargetInfo;
    public uint Flags;
}

[StructLayout(LayoutKind.Sequential)]
public struct DisplayConfigVideoSignalInfo
{
    public ulong PixelRate;
    public DisplayConfigRational HSyncFreq;
    public DisplayConfigRational VSyncFreq;
    public DisplayConfig2DRegion ActiveSize;
    public DisplayConfig2DRegion TotalSize;
    public uint VideoStandard;
    public uint ScanLineOrdering;
}

[StructLayout(LayoutKind.Sequential)]
public struct DisplayConfigTargetMode
{
    public DisplayConfigVideoSignalInfo TargetVideoSignalInfo;
}

[StructLayout(LayoutKind.Sequential)]
public struct PointL
{
    public int X;
    public int Y;
}

[StructLayout(LayoutKind.Sequential)]
public struct DisplayConfigSourceMode
{
    public uint Width;
    public uint Height;
    public uint PixelFormat;
    public PointL Position;
}

[StructLayout(LayoutKind.Sequential)]
public struct RectL
{
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
}

[StructLayout(LayoutKind.Sequential)]
public struct DisplayConfigDesktopImageInfo
{
    public PointL PathSourceSize;
    public RectL DesktopImageRegion;
    public RectL DesktopImageClip;
}

[StructLayout(LayoutKind.Explicit, Size = 64)]
public struct DisplayConfigModeInfo
{
    [FieldOffset(0)] public uint InfoType;
    [FieldOffset(4)] public uint Id;
    [FieldOffset(8)] public Luid AdapterId;
    [FieldOffset(16)] public DisplayConfigTargetMode TargetMode;
    [FieldOffset(16)] public DisplayConfigSourceMode SourceMode;
    [FieldOffset(16)] public DisplayConfigDesktopImageInfo DesktopImageInfo;
}

[StructLayout(LayoutKind.Sequential)]
public struct DisplayConfigDeviceInfoHeader
{
    public uint Type;
    public uint Size;
    public Luid AdapterId;
    public uint Id;
}

[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct DisplayConfigTargetDeviceName
{
    public DisplayConfigDeviceInfoHeader Header;
    public uint Flags;
    public uint OutputTechnology;
    public ushort EdidManufactureId;
    public ushort EdidProductCodeId;
    public uint ConnectorInstance;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)]
    public string MonitorFriendlyDeviceName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
    public string MonitorDevicePath;
}

[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct DisplayConfigAdapterName
{
    public DisplayConfigDeviceInfoHeader Header;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
    public string AdapterDevicePath;
}

[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct DisplayConfigSourceDeviceName
{
    public DisplayConfigDeviceInfoHeader Header;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
    public string ViewGdiDeviceName;
}
