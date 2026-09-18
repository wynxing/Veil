using Veil.Engine;

namespace Veil.Engine.Tests;

public sealed class RolesTests
{
    [Fact]
    public void VirtualNeedlesWinOverInternalTechnology()
    {
        var role = Roles.Classify(false, true, @"ROOT\DISPLAY\0000", @"\\?\DISPLAY#GVV0001", "GameViewer", @"\\.\DISPLAY1");
        Assert.Equal(PathRole.Virtual, role);
        Assert.False(Roles.IsBundledVdd(@"ROOT\DISPLAY\0000", @"\\?\DISPLAY#GVV0001", "GameViewer"));
    }

    [Fact]
    public void PlaceholderIsNotPhysical()
    {
        var role = Roles.Classify(true, false, "", @"\\?\DISPLAY#DEFAULT_MONITOR#1", "", "");
        Assert.Equal(PathRole.Placeholder, role);
    }

    [Fact]
    public void GameViewerIsVirtualButNotBundled()
    {
        var role = Roles.Classify(false, false, @"ROOT\DISPLAY\0000", @"\\?\DISPLAY#GVV0001", "GameViewer", @"\\.\DISPLAY3");
        Assert.Equal(PathRole.Virtual, role);
        Assert.False(Roles.IsBundledVdd(@"ROOT\DISPLAY\0000", @"\\?\DISPLAY#GVV0001", "GameViewer"));
    }

    [Fact]
    public void MttVddHardwarePathIsBundledWithoutFriendlyName()
    {
        Assert.True(Roles.IsBundledVdd(@"ROOT#MttVDD\0000", @"\\?\DISPLAY#ABC123#1", "Generic Monitor"));
        Assert.False(Roles.IsBundledVdd(@"ROOT\DISPLAY\0000", @"\\?\DISPLAY#GVV0001", "VDD by MTT"));
    }

    [Fact]
    public void MttVddIsBundledVirtual()
    {
        var role = Roles.Classify(false, false, @"ROOT\MttVDD\0000", @"\\?\DISPLAY#MTT1337#1", "Generic Monitor (VDD by MTT)", @"\\.\DISPLAY21");
        Assert.Equal(PathRole.Virtual, role);
        Assert.True(Roles.IsBundledVdd(@"ROOT\MttVDD\0000", @"\\?\DISPLAY#MTT1337#1", "Generic Monitor (VDD by MTT)"));
    }

    [Fact]
    public void ExternalDpIsPhysical()
    {
        var role = Roles.Classify(false, false, @"PCI\VEN_8086", @"\\?\DISPLAY#PDA0238#1", "S24Q6-Q24G8", @"\\.\DISPLAY1");
        Assert.Equal(PathRole.External, role);
    }
}
