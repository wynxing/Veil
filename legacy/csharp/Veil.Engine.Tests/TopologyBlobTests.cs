using Veil.Engine.Native;
using Veil.Engine.Topology;

namespace Veil.Engine.Tests;

public sealed class TopologyBlobTests
{
    [Fact]
    public void RoundTripPreservesPathBytes()
    {
        var paths = new[]
        {
            PathFactory.Path(id: 1),
            PathFactory.Path(internalTech: false, id: 2),
        };
        var modes = new[] { new DisplayConfigModeInfo { InfoType = 1, Id = 7 } };
        var blob = TopologyBlob.From(paths, modes);
        var (outPaths, outModes) = blob.ToArrays();
        Assert.Equal(TopologyBlob.StructBytes(paths), TopologyBlob.StructBytes(outPaths));
        Assert.Equal(TopologyBlob.StructBytes(modes), TopologyBlob.StructBytes(outModes));
        Assert.Equal(TopologyBlob.Fingerprint(paths, modes), TopologyBlob.Fingerprint(outPaths, outModes));
    }

    [Fact]
    public void SizeMismatchThrows()
    {
        var blob = TopologyBlob.From([PathFactory.Path()], []);
        blob.PathCount = 2;
        Assert.Throws<InvalidOperationException>(() => blob.ToArrays());
    }
}
