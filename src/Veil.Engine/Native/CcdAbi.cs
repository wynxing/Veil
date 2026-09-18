using System.Runtime.InteropServices;

namespace Veil.Engine.Native;

public static class CcdAbi
{
    public static IReadOnlyDictionary<string, int> Sizes { get; } = new Dictionary<string, int>
    {
        ["LUID"] = Marshal.SizeOf<Luid>(),
        ["DISPLAYCONFIG_PATH_SOURCE_INFO"] = Marshal.SizeOf<DisplayConfigPathSourceInfo>(),
        ["DISPLAYCONFIG_PATH_TARGET_INFO"] = Marshal.SizeOf<DisplayConfigPathTargetInfo>(),
        ["DISPLAYCONFIG_PATH_INFO"] = Marshal.SizeOf<DisplayConfigPathInfo>(),
        ["DISPLAYCONFIG_VIDEO_SIGNAL_INFO"] = Marshal.SizeOf<DisplayConfigVideoSignalInfo>(),
        ["DISPLAYCONFIG_MODE_INFO"] = Marshal.SizeOf<DisplayConfigModeInfo>(),
    };

    public static int ModeUnionOffset => Marshal.OffsetOf<DisplayConfigModeInfo>(nameof(DisplayConfigModeInfo.SourceMode)).ToInt32();

    public static void EnsureExpectedLayout()
    {
        var expected = new Dictionary<string, int>
        {
            ["LUID"] = 8,
            ["DISPLAYCONFIG_PATH_SOURCE_INFO"] = 20,
            ["DISPLAYCONFIG_PATH_TARGET_INFO"] = 48,
            ["DISPLAYCONFIG_PATH_INFO"] = 72,
            ["DISPLAYCONFIG_VIDEO_SIGNAL_INFO"] = 48,
            ["DISPLAYCONFIG_MODE_INFO"] = 64,
        };
        foreach (var pair in expected)
        {
            if (Sizes[pair.Key] != pair.Value)
            {
                throw new InvalidOperationException($"unexpected Windows ABI layout for {pair.Key}: {Sizes[pair.Key]} != {pair.Value}");
            }
        }

        if (IntPtr.Size != 8 || ModeUnionOffset != 16)
        {
            throw new InvalidOperationException($"unexpected Windows ABI layout: pointer={IntPtr.Size}, modeOffset={ModeUnionOffset}");
        }
    }
}
