using Veil.Engine;
using Veil.Engine.Native;
using Veil.Engine.Session;

namespace Veil.App.Tests;

public sealed class ScreenListBuilderTests
{
    [Fact]
    public void VirtualScreensAreNotListed()
    {
        var snap = new DisplaySnapshot(
        [
            Row(PathRole.Internal, 1, "Panel", @"PCI\VEN_8086", @"\\?\DISPLAY#CMN#1"),
            Row(PathRole.Virtual, 2, "VDD by MTT", @"ROOT\MttVDD\0000", @"\\?\DISPLAY#MTT1337#1"),
            Row(PathRole.External, 3, "S24", @"PCI\VEN_8086", @"\\?\DISPLAY#PDA#1"),
        ]);
        var items = ScreenListBuilder.Build(snap, null, [], false, true, true);
        Assert.Equal(new[] { "Panel", "S24" }, items.Select(i => i.Name));
        Assert.DoesNotContain(items, i => i.Name.Contains("VDD"));
    }

    [Fact]
    public void LastPhysicalIsDisabledWithReason()
    {
        var snap = new DisplaySnapshot(
        [
            Row(PathRole.Internal, 1, "Panel", @"PCI\VEN_8086", @"\\?\DISPLAY#CMN#1"),
        ]);
        var items = ScreenListBuilder.Build(snap, null, [], false, true, true);
        Assert.False(items[0].CanKeepOff);
        Assert.Contains("第二活动目标", items[0].BlockReason);
    }

    [Fact]
    public void FailureIsNeverShownAsClosed()
    {
        var snap = new DisplaySnapshot(
        [
            Row(PathRole.Internal, 1, "Panel", @"PCI\VEN_8086", @"\\?\DISPLAY#CMN#1"),
            Row(PathRole.External, 2, "S24", @"PCI\VEN_8086", @"\\?\DISPLAY#PDA#1"),
        ]);
        var hb = new HeartbeatFile
        {
            HotkeyRegistered = true,
            Screens =
            [
                new HeartbeatScreen
                {
                    AdapterLuid = "0000000000000001",
                    TargetId = 1,
                    MonitorPath = @"\\?\DISPLAY#CMN#1",
                    Name = "Panel",
                    Wanted = "保持关闭",
                    Confirmed = "失败",
                    Detail = "校验 87",
                },
            ],
        };
        var items = ScreenListBuilder.Build(snap, hb, [], false, true, true);
        var panel = items.Single(i => i.Name == "Panel");
        Assert.Equal("失败", panel.Confirmed);
        Assert.NotEqual("已关闭", panel.Confirmed);
        Assert.Contains("87", panel.BlockReason);
    }

    [Fact]
    public void HotkeyUnavailableBlocksKeepOff()
    {
        var snap = new DisplaySnapshot(
        [
            Row(PathRole.Internal, 1, "Panel", @"PCI\VEN_8086", @"\\?\DISPLAY#CMN#1"),
            Row(PathRole.External, 2, "S24", @"PCI\VEN_8086", @"\\?\DISPLAY#PDA#1"),
        ]);
        var items = ScreenListBuilder.Build(snap, null, [], false, true, false);
        Assert.All(items, i => Assert.False(i.CanKeepOff));
        Assert.Contains("热键", items[0].BlockReason);
    }

    [Fact]
    public void HeartbeatKeepsClosedScreenWhenSnapshotOmitsIt()
    {
        var snap = new DisplaySnapshot(
        [
            Row(PathRole.External, 2, "S24", @"PCI\VEN_8086", @"\\?\DISPLAY#PDA#1"),
        ]);
        var hb = new HeartbeatFile
        {
            Screens =
            [
                new HeartbeatScreen
                {
                    AdapterLuid = "0000000000000001",
                    TargetId = 1,
                    MonitorPath = @"\\?\DISPLAY#CMN#1",
                    Name = "Panel",
                    Wanted = "保持关闭",
                    Confirmed = "已关闭",
                },
            ],
        };
        var items = ScreenListBuilder.Build(snap, hb, [], false, true, true);
        Assert.Contains(items, i => i.Name == "Panel" && i.Confirmed == "已关闭" && i.CanRestore);
        Assert.Contains(items, i => i.Name == "S24");
    }

    [Fact]
    public void ClearedHeartbeatAfterRestoreShowsActiveScreensOn()
    {
        var snap = new DisplaySnapshot(
        [
            Row(PathRole.Internal, 1, "Panel", @"PCI\VEN_8086", @"\\?\DISPLAY#CMN#1"),
            Row(PathRole.External, 2, "S24", @"PCI\VEN_8086", @"\\?\DISPLAY#PDA#1"),
        ]);
        var items = ScreenListBuilder.Build(snap, heartbeat: null, pendingWanted: [], false, true, true);
        Assert.All(items, i =>
        {
            Assert.Equal("开启", i.Wanted);
            Assert.Equal("已显示", i.Confirmed);
            Assert.True(i.CanKeepOff);
            Assert.False(i.CanRestore);
        });
    }

    private static PathRow Row(PathRole role, uint id, string name, string adapter, string monitor) =>
        new(
            (int)id,
            true,
            CcdConstants.DisplayConfigPathActive,
            id,
            id,
            "0000000000000001",
            role == PathRole.Internal ? CcdConstants.OutputTechnologyInternal : 5,
            role == PathRole.Internal,
            @"\\.\DISPLAY" + id,
            adapter,
            name,
            monitor,
            false,
            role);
}
