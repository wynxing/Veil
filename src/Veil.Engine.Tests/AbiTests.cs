using System.Runtime.InteropServices;
using Veil.Engine.Native;

namespace Veil.Engine.Tests;

public sealed class AbiTests
{
    [Fact]
    public void PathAndModeSizesMatchProbe()
    {
        Assert.Equal(8, Marshal.SizeOf<Luid>());
        Assert.Equal(20, Marshal.SizeOf<DisplayConfigPathSourceInfo>());
        Assert.Equal(48, Marshal.SizeOf<DisplayConfigPathTargetInfo>());
        Assert.Equal(72, Marshal.SizeOf<DisplayConfigPathInfo>());
        Assert.Equal(48, Marshal.SizeOf<DisplayConfigVideoSignalInfo>());
        Assert.Equal(64, Marshal.SizeOf<DisplayConfigModeInfo>());
        Assert.Equal(16, CcdAbi.ModeUnionOffset);
        Assert.Equal(8, IntPtr.Size);
        CcdAbi.EnsureExpectedLayout();
    }

    [Fact]
    public void QueryAndSetFlagsMatchProbe()
    {
        Assert.Equal(82u, CcdConstants.QueryFlags);
        Assert.Equal(164960u, CcdConstants.ValidateFlags);
        Assert.Equal(0x80u, CcdConstants.SdcApply);
        Assert.Equal(0x200u, CcdConstants.SdcSaveToDatabase);
        Assert.Equal(0x4007u, CcdConstants.HotkeyModifiers);
        Assert.Equal(0x79u, CcdConstants.VkF10);
    }
}
