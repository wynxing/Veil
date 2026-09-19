using Veil.Engine.Native;
using Veil.Engine.Topology;

namespace Veil.Engine;

public sealed record ValidatePlanResult(int Rc, int DisabledCount, int RemainingActive, bool AdjustedOrigin, bool UsedApply)
{
    public bool Ok => Rc == 0 && DisabledCount > 0 && RemainingActive > 0 && !UsedApply;
}

public static class DisplayPlanner
{
    public static ValidatePlanResult ValidateDeactivate(
        ICcdApi ccd,
        IReadOnlyList<ScreenIdentity> selected,
        bool adjustOrigin)
    {
        var frame = ccd.Capture();
        var paths = frame.Paths;
        var modes = frame.Modes;
        var identities = frame.Snapshot.Paths.Select(p => p.Identity).ToList();
        if (identities.Count != paths.Length)
        {
            identities = Enumerable.Range(0, paths.Length)
                .Select(i => new ScreenIdentity(paths[i].TargetInfo.AdapterId.ToHex(), paths[i].TargetInfo.Id, ""))
                .ToList();
        }

        var prepared = PathOps.Deactivate(paths, modes, identities, selected, adjustOrigin);
        if (!prepared.CanApply)
        {
            return new ValidatePlanResult(CcdConstants.ErrorSuccess, prepared.DisabledCount, prepared.RemainingActive, prepared.AdjustedOrigin, false);
        }

        var flags = CcdConstants.ValidateFlags;
        if ((flags & CcdConstants.SdcApply) != 0)
        {
            throw new InvalidOperationException("VALIDATE wrapper must never include SDC_APPLY");
        }

        var rc = ccd.Set(prepared.Paths, prepared.Modes, flags);
        return new ValidatePlanResult(rc, prepared.DisabledCount, prepared.RemainingActive, prepared.AdjustedOrigin, false);
    }
}
