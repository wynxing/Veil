using Veil.Engine;
using Veil.Engine.Native;
using Veil.Engine.Topology;

namespace Veil.Engine.Tests;

internal static class PathFactory
{
    public static DisplayConfigPathInfo Path(bool internalTech = true, bool active = true, uint id = 1, uint modeIdx = 0x0001FFFF, uint adapterLow = 1)
    {
        var item = new DisplayConfigPathInfo
        {
            Flags = (active ? CcdConstants.DisplayConfigPathActive : 0u) | 8u,
        };
        item.TargetInfo.OutputTechnology = internalTech
            ? CcdConstants.OutputTechnologyInternal
            : 5u;
        item.TargetInfo.Id = id;
        item.TargetInfo.AdapterId = new Luid { LowPart = adapterLow, HighPart = 0 };
        item.SourceInfo.Id = id;
        item.SourceInfo.AdapterId = item.TargetInfo.AdapterId;
        item.SourceInfo.ModeInfoIdx = modeIdx;
        item.TargetInfo.ModeInfoIdx = 0x00020003;
        return item;
    }

    public static ScreenIdentity Id(uint targetId, string adapter = "0000000000000001", string monitor = "") =>
        new(adapter, targetId, monitor);

    public static PathRow Row(
        PathRole role,
        bool active = true,
        uint targetId = 1,
        string name = "Panel",
        string adapterPath = @"PCI\VEN_8086",
        string monitorPath = @"\\?\DISPLAY#CMN1540#1",
        string adapterLuid = "0000000000000001")
    {
        var internalTech = role == PathRole.Internal;
        return new PathRow(
            Index: (int)targetId,
            Active: active,
            Flags: active ? CcdConstants.DisplayConfigPathActive : 0,
            SourceId: targetId,
            TargetId: targetId,
            AdapterLuid: adapterLuid,
            OutputTechnology: internalTech ? CcdConstants.OutputTechnologyInternal : 5,
            Internal: internalTech,
            SourceName: @"\\.\DISPLAY" + targetId,
            AdapterPath: adapterPath,
            MonitorName: name,
            MonitorPath: monitorPath,
            Placeholder: monitorPath.Contains("DEFAULT_MONITOR", StringComparison.OrdinalIgnoreCase),
            Role: role);
    }
}

internal sealed class FakeCcd : ICcdApi
{
    public DisplayConfigPathInfo[] Paths { get; set; } = [];
    public DisplayConfigModeInfo[] Modes { get; set; } = [];
    public List<PathRow> Rows { get; set; } = [];
    public int ValidateRc { get; set; }
    public int ApplyRc { get; set; }
    public int? NextApplyRc { get; set; }
    public int CloneRc { get; set; }
    public int InternalRc { get; set; }
    public List<uint> Flags { get; } = [];

    public (DisplayConfigPathInfo[] Paths, DisplayConfigModeInfo[] Modes) QueryRaw(uint flags = CcdConstants.QueryFlags) =>
        (ClonePaths(), CloneModes());

    public CcdFrame Capture(uint flags = CcdConstants.QueryFlags) =>
        new(ClonePaths(), CloneModes(), new DisplaySnapshot(Rows.ToList()));

    public DisplaySnapshot QuerySnapshot(uint flags = CcdConstants.QueryFlags) => Capture(flags).Snapshot;

    public int Set(DisplayConfigPathInfo[] paths, DisplayConfigModeInfo[] modes, uint flags)
    {
        Flags.Add(flags);
        if ((flags & CcdConstants.SdcSaveToDatabase) != 0)
        {
            throw new InvalidOperationException("SAVE_TO_DATABASE");
        }

        if ((flags & CcdConstants.SdcApply) != 0)
        {
            Paths = (DisplayConfigPathInfo[])paths.Clone();
            Modes = (DisplayConfigModeInfo[])modes.Clone();
            SyncRowsFromPaths();
            var rc = NextApplyRc ?? ApplyRc;
            NextApplyRc = null;
            return rc;
        }

        return ValidateRc;
    }

    public int SetTopology(uint topologyFlags)
    {
        Flags.Add(topologyFlags);
        if ((topologyFlags & CcdConstants.SdcTopologyClone) != 0)
        {
            return CloneRc;
        }

        return InternalRc;
    }

    public string? GetLastErrorMessage(int code) => code.ToString();

    public bool Applied => Flags.Any(f => (f & CcdConstants.SdcApply) != 0);

    public bool Validated => Flags.Any(f => (f & CcdConstants.SdcValidate) != 0);

    private DisplayConfigPathInfo[] ClonePaths() => (DisplayConfigPathInfo[])Paths.Clone();

    private DisplayConfigModeInfo[] CloneModes() => (DisplayConfigModeInfo[])Modes.Clone();

    private void SyncRowsFromPaths()
    {
        if (Rows.Count != Paths.Length)
        {
            return;
        }

        for (var i = 0; i < Paths.Length; i++)
        {
            var active = (Paths[i].Flags & CcdConstants.DisplayConfigPathActive) != 0;
            var row = Rows[i];
            Rows[i] = row with { Active = active, Flags = Paths[i].Flags };
        }
    }
}

internal sealed class FakeHotkey : IHotkey
{
    public bool RegisterSuccess { get; set; } = true;
    public bool Pressed { get; set; }
    public bool Registered { get; private set; }
    public int RegisterCalls { get; private set; }

    public bool TryRegister()
    {
        RegisterCalls++;
        Registered = RegisterSuccess;
        return RegisterSuccess;
    }

    public void Unregister() => Registered = false;

    public bool WasPressed()
    {
        if (!Pressed)
        {
            return false;
        }

        Pressed = false;
        return true;
    }
}

internal sealed class FakeClock : IMonotonicClock
{
    public double Seconds { get; set; }
}

internal sealed class FakeParent : IParentWatcher
{
    public bool Alive { get; set; } = true;

    public bool IsAlive(int pid) => Alive;
}
