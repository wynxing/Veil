using Veil.Engine.Native;

namespace Veil.Engine.Topology;

public sealed record DeactivateResult(
    DisplayConfigPathInfo[] Paths,
    DisplayConfigModeInfo[] Modes,
    int DisabledCount,
    int RemainingActive,
    bool AdjustedOrigin)
{
    public bool CanApply => DisabledCount > 0 && RemainingActive > 0;
}

public static class PathOps
{
    public static DeactivateResult Deactivate(
        DisplayConfigPathInfo[] paths,
        DisplayConfigModeInfo[] modes,
        IReadOnlyList<ScreenIdentity> pathIdentities,
        IReadOnlyList<ScreenIdentity> selected,
        bool adjustOrigin)
    {
        if (paths.Length != pathIdentities.Count)
        {
            throw new ArgumentException("path identity count must match paths.");
        }

        var changed = new DisplayConfigPathInfo[paths.Length];
        Array.Copy(paths, changed, paths.Length);
        var disabled = 0;
        var remaining = 0;
        for (var i = 0; i < changed.Length; i++)
        {
            var active = (changed[i].Flags & CcdConstants.DisplayConfigPathActive) != 0;
            var selectedMatch = selected.Any(id => id.Matches(pathIdentities[i]));
            if (active && selectedMatch)
            {
                changed[i].Flags &= ~CcdConstants.DisplayConfigPathActive;
                disabled++;
            }
            else if ((changed[i].Flags & CcdConstants.DisplayConfigPathActive) != 0)
            {
                remaining++;
            }
        }

        var outModes = modes;
        var moved = false;
        if (adjustOrigin && remaining > 0)
        {
            (outModes, moved) = MoveRemainingToOrigin(changed, modes);
        }

        return new DeactivateResult(changed, outModes, disabled, remaining, moved);
    }

    public static (DisplayConfigModeInfo[] Modes, bool Moved) MoveRemainingToOrigin(
        DisplayConfigPathInfo[] paths,
        DisplayConfigModeInfo[] modes)
    {
        var changed = new DisplayConfigModeInfo[modes.Length];
        Array.Copy(modes, changed, modes.Length);
        var moved = false;
        var remainingActive = paths.Where(p => (p.Flags & CcdConstants.DisplayConfigPathActive) != 0).ToList();
        var alreadyAtOrigin = false;
        foreach (var path in remainingActive)
        {
            var idx = SourceModeIndex(path, changed);
            if (idx is null)
            {
                continue;
            }

            if (changed[idx.Value].SourceMode.Position.X == 0 && changed[idx.Value].SourceMode.Position.Y == 0)
            {
                alreadyAtOrigin = true;
                break;
            }
        }

        if (alreadyAtOrigin)
        {
            return (changed, false);
        }

        foreach (var path in remainingActive)
        {
            var idx = SourceModeIndex(path, changed);
            if (idx is null)
            {
                continue;
            }

            var mode = changed[idx.Value];
            var source = mode.SourceMode;
            source.Position = new PointL { X = 0, Y = 0 };
            mode.SourceMode = source;
            changed[idx.Value] = mode;
            moved = true;
            break;
        }

        return (changed, moved);
    }

    public static int? SourceModeIndex(DisplayConfigPathInfo path, DisplayConfigModeInfo[] modes)
    {
        var packed = path.SourceInfo.ModeInfoIdx;
        var packedSrc = (packed >> 16) & 0xFFFF;
        foreach (var idx in new[] { packedSrc, packed })
        {
            if (idx == CcdConstants.DisplayConfigPathSourceModeIdxInvalid || idx >= modes.Length)
            {
                continue;
            }

            if (modes[idx].InfoType == CcdConstants.DisplayConfigModeInfoTypeSource)
            {
                return (int)idx;
            }
        }

        return null;
    }

    public static List<(string Adapter, uint TargetId)> ActiveTargets(DisplayConfigPathInfo[] paths)
    {
        return paths
            .Where(p => (p.Flags & CcdConstants.DisplayConfigPathActive) != 0)
            .Select(p => (p.TargetInfo.AdapterId.ToHex(), p.TargetInfo.Id))
            .OrderBy(x => x.Item1)
            .ThenBy(x => x.Id)
            .ToList();
    }
}
