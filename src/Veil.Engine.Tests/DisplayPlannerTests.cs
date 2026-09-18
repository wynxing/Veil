using Veil.Engine;
using Veil.Engine.Native;

namespace Veil.Engine.Tests;

public sealed class DisplayPlannerTests
{
    [Fact]
    public void ValidateNeverSetsApply()
    {
        var internalPath = PathFactory.Path(id: 1);
        var externalPath = PathFactory.Path(internalTech: false, id: 2);
        var ccd = new FakeCcd
        {
            Paths = [internalPath, externalPath],
            Rows =
            [
                PathFactory.Row(PathRole.Internal, targetId: 1),
                PathFactory.Row(PathRole.External, targetId: 2, name: "S24", monitorPath: @"\\?\DISPLAY#PDA#1"),
            ],
        };
        var result = DisplayPlanner.ValidateDeactivate(ccd, [ccd.Rows[0].Identity], adjustOrigin: true);
        Assert.True(result.Ok);
        Assert.False(result.UsedApply);
        Assert.All(ccd.Flags, flags => Assert.Equal(0u, flags & CcdConstants.SdcApply));
        Assert.All(ccd.Flags, flags => Assert.NotEqual(0u, flags & CcdConstants.SdcValidate));
        Assert.DoesNotContain(ccd.Flags, flags => (flags & CcdConstants.SdcSaveToDatabase) != 0);
    }

    [Fact]
    public void ZeroRemainingDoesNotCountAsOk()
    {
        var ccd = new FakeCcd
        {
            Paths = [PathFactory.Path(id: 1)],
            Rows = [PathFactory.Row(PathRole.Internal, targetId: 1)],
        };
        var result = DisplayPlanner.ValidateDeactivate(ccd, [ccd.Rows[0].Identity], adjustOrigin: false);
        Assert.False(result.Ok);
        Assert.Equal(0, result.RemainingActive);
        Assert.Empty(ccd.Flags);
    }
}
