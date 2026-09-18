using Veil.Engine;
using Veil.Engine.Session;

namespace Veil.App;

public sealed class ScreenItem
{
    public required ScreenIdentity Identity { get; init; }
    public required string Name { get; init; }
    public required string Kind { get; init; }
    public required string Wanted { get; init; }
    public required string Confirmed { get; init; }
    public required bool CanKeepOff { get; init; }
    public required bool CanRestore { get; init; }
    public required string StatusText { get; init; }
    public required string BlockReason { get; init; }
}

public static class ScreenListBuilder
{
    public static IReadOnlyList<ScreenItem> Build(
        DisplaySnapshot snapshot,
        HeartbeatFile? heartbeat,
        IReadOnlyList<ScreenIdentity> pendingWanted,
        bool bundledVddInstalled,
        bool recoveryReady,
        bool hotkeyRegistered)
    {
        var items = new List<ScreenItem>();
        foreach (var row in snapshot.PhysicalScreens)
        {
            var hb = heartbeat?.Screens.FirstOrDefault(s =>
                row.Identity.Matches(new ScreenIdentity(s.AdapterLuid, s.TargetId, s.MonitorPath)));
            var wanted = hb?.Wanted
                ?? (pendingWanted.Any(id => id.Matches(row.Identity)) ? "保持关闭" : "开启");
            var confirmed = hb?.Confirmed ?? (row.Active ? "已显示" : "未知");
            if (confirmed == "失败" && hb is null)
            {
                confirmed = "失败";
            }

            var already = pendingWanted.Concat(
                heartbeat?.Screens.Where(s => s.Wanted == "保持关闭")
                    .Select(s => new ScreenIdentity(s.AdapterLuid, s.TargetId, s.MonitorPath))
                ?? []).ToList();

            string? block = null;
            if (!recoveryReady)
            {
                block = "恢复进程未就绪。";
            }
            else if (!hotkeyRegistered)
            {
                block = "紧急热键不可用。";
            }
            else
            {
                block = Gate.ScreenKeepOffBlockReason(snapshot, row.Identity, already, bundledVddInstalled);
            }

            var canKeepOff = wanted != "保持关闭" && block is null;
            var canRestore = wanted == "保持关闭";
            items.Add(new ScreenItem
            {
                Identity = row.Identity,
                Name = row.DisplayName,
                Kind = row.Role == PathRole.Internal ? "内置" : "外接",
                Wanted = wanted,
                Confirmed = confirmed,
                CanKeepOff = canKeepOff,
                CanRestore = canRestore,
                StatusText = $"{confirmed}（{wanted}）",
                BlockReason = block ?? hb?.Detail ?? "",
            });
        }

        foreach (var hbRow in heartbeat?.Screens ?? [])
        {
            var id = new ScreenIdentity(hbRow.AdapterLuid, hbRow.TargetId, hbRow.MonitorPath);
            if (items.Any(i => i.Identity.Matches(id)))
            {
                continue;
            }

            if (hbRow.Wanted != "保持关闭" && hbRow.Confirmed != "已关闭")
            {
                continue;
            }

            items.Add(new ScreenItem
            {
                Identity = id,
                Name = string.IsNullOrWhiteSpace(hbRow.Name) ? "已关闭的物理屏" : hbRow.Name,
                Kind = "物理",
                Wanted = hbRow.Wanted,
                Confirmed = hbRow.Confirmed,
                CanKeepOff = false,
                CanRestore = hbRow.Wanted == "保持关闭" || hbRow.Confirmed == "已关闭",
                StatusText = $"{hbRow.Confirmed}（{hbRow.Wanted}）",
                BlockReason = hbRow.Detail ?? "",
            });
        }

        return items;
    }
}
