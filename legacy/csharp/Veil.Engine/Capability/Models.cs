namespace Veil.Engine;

public enum PathRole
{
    Internal,
    External,
    Virtual,
    Placeholder,
}

public readonly record struct ScreenIdentity(string AdapterLuid, uint TargetId, string MonitorPath)
{
    public bool Matches(ScreenIdentity other)
    {
        if (!string.Equals(AdapterLuid, other.AdapterLuid, StringComparison.OrdinalIgnoreCase)
            || TargetId != other.TargetId)
        {
            return false;
        }

        if (string.IsNullOrEmpty(MonitorPath) || string.IsNullOrEmpty(other.MonitorPath))
        {
            return true;
        }

        return string.Equals(MonitorPath, other.MonitorPath, StringComparison.OrdinalIgnoreCase);
    }

    public override string ToString() => $"{AdapterLuid}:{TargetId}:{MonitorPath}";
}

public sealed record PathRow(
    int Index,
    bool Active,
    uint Flags,
    uint SourceId,
    uint TargetId,
    string AdapterLuid,
    uint OutputTechnology,
    bool Internal,
    string SourceName,
    string AdapterPath,
    string MonitorName,
    string MonitorPath,
    bool Placeholder,
    PathRole Role,
    ushort EdidManufactureId = 0,
    ushort EdidProductCodeId = 0)
{
    public ScreenIdentity Identity => new(AdapterLuid, TargetId, MonitorPath);

    public bool IsPhysical => Role is PathRole.Internal or PathRole.External;

    public bool IsBundledVdd => Role == PathRole.Virtual && Roles.IsBundledVdd(AdapterPath, MonitorPath, MonitorName);

    public string DisplayName =>
        string.IsNullOrWhiteSpace(MonitorName) ? (string.IsNullOrWhiteSpace(SourceName) ? "未命名" : SourceName) : MonitorName;
}

public sealed class DisplaySnapshot
{
    public DisplaySnapshot(IReadOnlyList<PathRow> paths, int gdiMonitorCount = 0)
    {
        Paths = paths;
        GdiMonitorCount = gdiMonitorCount;
    }

    public IReadOnlyList<PathRow> Paths { get; }
    public int GdiMonitorCount { get; }

    public IEnumerable<PathRow> ActivePaths => Paths.Where(p => p.Active);

    public IEnumerable<PathRow> PhysicalScreens => Paths.Where(p => p.IsPhysical);

    public IEnumerable<PathRow> ActivePhysical => Paths.Where(p => p.Active && p.IsPhysical);

    public bool HasActiveBundledVdd => Paths.Any(p => p.Active && p.IsBundledVdd);

    public bool HasActiveThirdPartyVirtual =>
        Paths.Any(p => p.Active && p.Role == PathRole.Virtual && !p.IsBundledVdd);
}
