namespace Veil.Engine;

public enum KeepOffAction
{
    None,
    Deactivate,
    EnableBundledVdd,
    Blocked,
}

public sealed record KeepOffPlan(
    KeepOffAction Action,
    string? BlockReason,
    bool AdjustOrigin,
    bool NeedsBundledVdd,
    bool MayAdjustClone,
    int SelectedActiveCount,
    int RemainingPhysicalActive)
{
    public bool IsAllowed => Action is KeepOffAction.Deactivate or KeepOffAction.EnableBundledVdd;

    public static KeepOffPlan None(string detail) =>
        new(KeepOffAction.None, detail, false, false, false, 0, 0);

    public static KeepOffPlan Block(string reason) =>
        new(KeepOffAction.Blocked, reason, false, false, false, 0, 0);
}

public static class Gate
{
    public const string LastPathReason = "没有第二活动目标（其它物理屏或自带 VDD），无法停用最后一条物理路径。";
    public const string EnableVddReason = "将启用安装器自带的隐藏虚拟输出，显示拓扑可能短暂变化。";
    public const string ThirdPartyVirtualIgnored = "第三方虚拟屏不能作为第二目标。";

    public static KeepOffPlan PlanKeepOff(DisplaySnapshot snapshot, IReadOnlyList<ScreenIdentity> selected, bool bundledVddInstalled = false)
    {
        if (selected.Count == 0)
        {
            return KeepOffPlan.None("未选择物理屏。");
        }

        var activePhysical = snapshot.ActivePhysical.ToList();
        var turningOff = activePhysical.Where(row => selected.Any(id => id.Matches(row.Identity))).ToList();
        var remainingPhysical = activePhysical.Count(row => !selected.Any(id => id.Matches(row.Identity)));
        if (turningOff.Count == 0)
        {
            if (snapshot.ActivePaths.Any())
            {
                return new KeepOffPlan(
                    KeepOffAction.Deactivate,
                    null,
                    AdjustOrigin: false,
                    NeedsBundledVdd: false,
                    MayAdjustClone: snapshot.HasActiveBundledVdd && remainingPhysical == 0,
                    0,
                    remainingPhysical);
            }

            return KeepOffPlan.Block("没有可关闭的已连接物理屏。");
        }
        if (remainingPhysical >= 1)
        {
            return new KeepOffPlan(
                KeepOffAction.Deactivate,
                null,
                AdjustOrigin: true,
                NeedsBundledVdd: false,
                MayAdjustClone: false,
                turningOff.Count,
                remainingPhysical);
        }

        if (snapshot.HasActiveBundledVdd)
        {
            return new KeepOffPlan(
                KeepOffAction.Deactivate,
                null,
                AdjustOrigin: true,
                NeedsBundledVdd: false,
                MayAdjustClone: true,
                turningOff.Count,
                0);
        }

        if (bundledVddInstalled)
        {
            return new KeepOffPlan(
                KeepOffAction.EnableBundledVdd,
                EnableVddReason,
                AdjustOrigin: true,
                NeedsBundledVdd: true,
                MayAdjustClone: true,
                turningOff.Count,
                0);
        }

        return KeepOffPlan.Block(LastPathReason);
    }

    public static string? ScreenKeepOffBlockReason(
        DisplaySnapshot snapshot,
        ScreenIdentity screen,
        IReadOnlyList<ScreenIdentity> alreadyWanted,
        bool bundledVddInstalled = false)
    {
        var selected = alreadyWanted.Concat([screen]).DistinctBy(x => (x.AdapterLuid, x.TargetId, x.MonitorPath)).ToList();
        var plan = PlanKeepOff(snapshot, selected, bundledVddInstalled);
        return plan.Action == KeepOffAction.Blocked ? plan.BlockReason : null;
    }
}
