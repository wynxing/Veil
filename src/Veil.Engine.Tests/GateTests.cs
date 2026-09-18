using Veil.Engine;

namespace Veil.Engine.Tests;

public sealed class GateTests
{
    [Fact]
    public void LastPhysicalWithoutBundledVddIsBlocked()
    {
        var snap = new DisplaySnapshot([PathFactory.Row(PathRole.Internal)]);
        var plan = Gate.PlanKeepOff(snap, [PathFactory.Id(1, monitor: @"\\?\DISPLAY#CMN1540#1")]);
        Assert.Equal(KeepOffAction.Blocked, plan.Action);
        Assert.Contains("第二活动目标", plan.BlockReason);
        Assert.False(plan.NeedsBundledVdd);
        Assert.False(plan.MayAdjustClone);
    }

    [Fact]
    public void GameViewerEvenWithInternalTechDoesNotUnlockLastPhysical()
    {
        var viewer = Roles.Classify(false, true, @"ROOT\DISPLAY\0000", @"\\?\DISPLAY#GVV#1", "GameViewer", @"\\.\DISPLAY3");
        Assert.Equal(PathRole.Virtual, viewer);
        var snap = new DisplaySnapshot(
        [
            PathFactory.Row(PathRole.Internal, targetId: 1),
            PathFactory.Row(PathRole.Virtual, targetId: 2, name: "GameViewer", adapterPath: @"ROOT\DISPLAY\0000", monitorPath: @"\\?\DISPLAY#GVV#1"),
        ]);
        var plan = Gate.PlanKeepOff(snap, [snap.Paths[0].Identity]);
        Assert.Equal(KeepOffAction.Blocked, plan.Action);
        Assert.DoesNotContain(snap.PhysicalScreens, p => p.Role == PathRole.Virtual);
    }

    [Fact]
    public void ThirdPartyVirtualDoesNotUnlockLastPhysical()
    {
        var snap = new DisplaySnapshot(
        [
            PathFactory.Row(PathRole.Internal, targetId: 1),
            PathFactory.Row(PathRole.Virtual, targetId: 2, name: "GameViewer", adapterPath: @"ROOT\DISPLAY\0000", monitorPath: @"\\?\DISPLAY#GV#1"),
        ]);
        var plan = Gate.PlanKeepOff(snap, [snap.Paths[0].Identity]);
        Assert.Equal(KeepOffAction.Blocked, plan.Action);
        Assert.False(snap.HasActiveBundledVdd);
        Assert.True(snap.HasActiveThirdPartyVirtual);
    }

    [Fact]
    public void BundledVddAllowsClosingAllPhysical()
    {
        var snap = new DisplaySnapshot(
        [
            PathFactory.Row(PathRole.Internal, targetId: 1),
            PathFactory.Row(PathRole.Virtual, targetId: 2, name: "VDD by MTT", adapterPath: @"ROOT\MttVDD\0000", monitorPath: @"\\?\DISPLAY#MTT1337#1"),
        ]);
        var plan = Gate.PlanKeepOff(snap, [snap.Paths[0].Identity]);
        Assert.Equal(KeepOffAction.Deactivate, plan.Action);
        Assert.True(plan.MayAdjustClone);
        Assert.True(snap.HasActiveBundledVdd);
    }

    [Fact]
    public void InstalledButInactiveVddRequestsEnable()
    {
        var snap = new DisplaySnapshot([PathFactory.Row(PathRole.Internal)]);
        var plan = Gate.PlanKeepOff(snap, [snap.Paths[0].Identity], bundledVddInstalled: true);
        Assert.Equal(KeepOffAction.EnableBundledVdd, plan.Action);
        Assert.True(plan.NeedsBundledVdd);
        Assert.Contains("隐藏虚拟输出", plan.BlockReason);
    }

    [Fact]
    public void TwoPhysicalAllowsNativeDeactivateWithoutClone()
    {
        var snap = new DisplaySnapshot(
        [
            PathFactory.Row(PathRole.Internal, targetId: 1, name: "Panel", monitorPath: @"\\?\DISPLAY#CMN#1"),
            PathFactory.Row(PathRole.External, targetId: 2, name: "S24", monitorPath: @"\\?\DISPLAY#PDA#1"),
        ]);
        var plan = Gate.PlanKeepOff(snap, [snap.Paths[0].Identity]);
        Assert.Equal(KeepOffAction.Deactivate, plan.Action);
        Assert.False(plan.MayAdjustClone);
        Assert.False(plan.NeedsBundledVdd);
        Assert.Equal(1, plan.RemainingPhysicalActive);
    }

    [Fact]
    public void PhysicalListOmitsVirtual()
    {
        var snap = new DisplaySnapshot(
        [
            PathFactory.Row(PathRole.Internal, targetId: 1),
            PathFactory.Row(PathRole.Virtual, targetId: 2, name: "VDD by MTT", adapterPath: @"ROOT\MttVDD\0000", monitorPath: @"\\?\DISPLAY#MTT1337#1"),
            PathFactory.Row(PathRole.External, targetId: 3, name: "S24", monitorPath: @"\\?\DISPLAY#PDA#1"),
        ]);
        Assert.Equal(new[] { PathRole.Internal, PathRole.External }, snap.PhysicalScreens.Select(p => p.Role));
    }
}
