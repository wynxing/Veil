using Veil.Engine;
using Veil.Engine.Native;
using Veil.Engine.Topology;

namespace Veil.Engine.Tests;

public sealed class PathOpsTests
{
    [Fact]
    public void DeactivationPreservesAuxiliaryUnionAndOriginal()
    {
        var original = new[]
        {
            PathFactory.Path(internalTech: true, active: true, id: 1),
            PathFactory.Path(internalTech: false, active: true, id: 2),
            PathFactory.Path(internalTech: true, active: false, id: 3),
        };
        var before = TopologyBlob.StructBytes(original);
        var identities = new[]
        {
            PathFactory.Id(1),
            PathFactory.Id(2),
            PathFactory.Id(3),
        };
        var result = PathOps.Deactivate(original, [], identities, [PathFactory.Id(1)], adjustOrigin: false);
        Assert.Equal(1, result.DisabledCount);
        Assert.Equal(1, result.RemainingActive);
        Assert.Equal(before, TopologyBlob.StructBytes(original));
        Assert.Equal(0u, result.Paths[0].Flags & CcdConstants.DisplayConfigPathActive);
        Assert.Equal(8u, result.Paths[0].Flags);
        Assert.Equal(0x0001FFFFu, result.Paths[0].SourceInfo.ModeInfoIdx);
        Assert.Equal(TopologyBlob.StructBytes([original[1]]), TopologyBlob.StructBytes([result.Paths[1]]));
    }

    [Fact]
    public void RemainingSourceMovesToOriginWithoutMutatingInput()
    {
        var remaining = PathFactory.Path(internalTech: true, active: true, id: 1, modeIdx: 1);
        var targetMode = new DisplayConfigModeInfo { InfoType = 2 };
        var sourceMode = new DisplayConfigModeInfo { InfoType = 1 };
        sourceMode.SourceMode = new DisplayConfigSourceMode
        {
            Width = 1920,
            Height = 1200,
            Position = new PointL { X = 2560, Y = 12 },
        };
        var original = TopologyBlob.StructBytes([sourceMode]);
        var (shifted, moved) = PathOps.MoveRemainingToOrigin([remaining], [targetMode, sourceMode]);
        Assert.True(moved);
        Assert.Equal(0, shifted[1].SourceMode.Position.X);
        Assert.Equal(0, shifted[1].SourceMode.Position.Y);
        Assert.Equal(original, TopologyBlob.StructBytes([sourceMode]));
    }

    [Fact]
    public void VirtualPackedSourceIndexMovesToOrigin()
    {
        var remaining = PathFactory.Path(internalTech: true, active: true, id: 1, modeIdx: (1u << 16) | 0xFFFF);
        var modes = new DisplayConfigModeInfo[2];
        modes[0].InfoType = 2;
        modes[1].InfoType = 1;
        modes[1].SourceMode = new DisplayConfigSourceMode { Position = new PointL { X = 2560, Y = 0 } };
        var (shifted, moved) = PathOps.MoveRemainingToOrigin([remaining], modes);
        Assert.True(moved);
        Assert.Equal(0, shifted[1].SourceMode.Position.X);
    }

    [Fact]
    public void ClosingPrimaryExternalMovesOrigin()
    {
        var paths = new[]
        {
            PathFactory.Path(internalTech: true, active: true, id: 1, modeIdx: 0),
            PathFactory.Path(internalTech: false, active: true, id: 2, modeIdx: 1),
        };
        var modes = new DisplayConfigModeInfo[2];
        modes[0].InfoType = 1;
        modes[0].SourceMode = new DisplayConfigSourceMode { Position = new PointL { X = 2560, Y = 0 } };
        modes[1].InfoType = 1;
        modes[1].SourceMode = new DisplayConfigSourceMode { Position = new PointL { X = 0, Y = 0 } };
        var identities = new[] { PathFactory.Id(1), PathFactory.Id(2) };
        var result = PathOps.Deactivate(paths, modes, identities, [PathFactory.Id(2)], adjustOrigin: true);
        Assert.True(result.AdjustedOrigin);
        Assert.Equal(0, result.Modes[0].SourceMode.Position.X);
        Assert.Equal(1, result.DisabledCount);
        Assert.Equal(1, result.RemainingActive);
    }

    [Fact]
    public void OriginAdjustMovesOnlyOneRemainingSource()
    {
        var paths = new[]
        {
            PathFactory.Path(internalTech: true, active: true, id: 1, modeIdx: 0),
            PathFactory.Path(internalTech: false, active: true, id: 2, modeIdx: 1),
            PathFactory.Path(internalTech: false, active: true, id: 3, modeIdx: 2, adapterLow: 2),
        };
        var modes = new DisplayConfigModeInfo[3];
        modes[0].InfoType = 1;
        modes[0].SourceMode = new DisplayConfigSourceMode { Position = new PointL { X = 100, Y = 0 } };
        modes[1].InfoType = 1;
        modes[1].SourceMode = new DisplayConfigSourceMode { Position = new PointL { X = 200, Y = 0 } };
        modes[2].InfoType = 1;
        modes[2].SourceMode = new DisplayConfigSourceMode { Position = new PointL { X = 300, Y = 0 } };
        var identities = new[] { PathFactory.Id(1), PathFactory.Id(2), PathFactory.Id(3, adapter: "0000000000000002") };
        var result = PathOps.Deactivate(paths, modes, identities, [PathFactory.Id(1)], adjustOrigin: true);
        Assert.True(result.AdjustedOrigin);
        var moved = result.Modes.Count(m => m.SourceMode.Position.X == 0 && m.SourceMode.Position.Y == 0);
        Assert.Equal(1, moved);
        Assert.Equal(2, result.RemainingActive);
    }
}
