using System.IO;
using Veil.Engine;
using Veil.Engine.Native;
using Veil.Engine.Session;

namespace Veil.App.Tests;

public sealed class RecoveryCoordinatorTests
{
    [Fact]
    public void FormatResultMapsRelease() =>
        Assert.Equal("已恢复全部。", RecoveryCoordinator.FormatResult(new ResultFile { Ok = true, Reason = "release" }));

    [Fact]
    public void FormatResultMapsHotkey() =>
        Assert.Equal("已由 Ctrl+Alt+Shift+F10 恢复。", RecoveryCoordinator.FormatResult(new ResultFile { Ok = true, Reason = "hotkey" }));

    [Fact]
    public void SessionResultClearsKeepOffHeartbeat()
    {
        var started = "";
        try
        {
            var ccd = DualPhysical();
            var coordinator = new RecoveryCoordinator(
                ccd,
                startRecovery: (sessionDir, _) =>
                {
                    started = sessionDir;
                    JsonUtil.WriteAtomic(SessionPaths.Ready(sessionDir), new ReadyFile
                    {
                        Pid = 4242,
                        HotkeyRegistered = true,
                    });
                    return 4242;
                },
                runDriverHelper: _ => 0);

            var identity = ccd.QuerySnapshot().PhysicalScreens.First(r => r.Role == PathRole.Internal).Identity;
            Assert.Null(coordinator.KeepOff(identity));
            Assert.True(coordinator.HasSession);
            Assert.NotEmpty(started);

            JsonUtil.WriteAtomic(SessionPaths.Heartbeat(started), new HeartbeatFile
            {
                HotkeyRegistered = true,
                Armed = true,
                Detail = "已保持关闭。",
                Screens =
                [
                    new HeartbeatScreen
                    {
                        AdapterLuid = identity.AdapterLuid,
                        TargetId = identity.TargetId,
                        MonitorPath = identity.MonitorPath,
                        Name = "Panel",
                        Wanted = "保持关闭",
                        Confirmed = "已关闭",
                        Detail = "已保持关闭。",
                    },
                ],
            });
            JsonUtil.WriteAtomic(SessionPaths.Result(started), new ResultFile
            {
                Ok = true,
                Reason = "release",
                RestoreRc = 0,
                RestoredTopology = true,
            });

            coordinator.Poll();
            Assert.False(coordinator.HasSession);
            Assert.Null(coordinator.Heartbeat);
            Assert.Empty(coordinator.Wanted);
            Assert.Equal("已恢复全部。", coordinator.StatusText);
            Assert.False(coordinator.HotkeyRegistered);
        }
        finally
        {
            if (!string.IsNullOrEmpty(started) && Directory.Exists(started))
            {
                try
                {
                    Directory.Delete(started, true);
                }
                catch (IOException)
                {
                }
            }
        }
    }

    private static StubCcd DualPhysical()
    {
        var internalPath = Path(internalTech: true, id: 1);
        var externalPath = Path(internalTech: false, id: 2);
        return new StubCcd
        {
            Paths = [internalPath, externalPath],
            Modes = [new DisplayConfigModeInfo { InfoType = 1 }],
            Rows =
            [
                Row(PathRole.Internal, 1, "Panel", @"\\?\DISPLAY#CMN#1"),
                Row(PathRole.External, 2, "S24", @"\\?\DISPLAY#PDA#1"),
            ],
        };
    }

    private static DisplayConfigPathInfo Path(bool internalTech, uint id)
    {
        var item = new DisplayConfigPathInfo
        {
            Flags = CcdConstants.DisplayConfigPathActive | 8u,
        };
        item.TargetInfo.OutputTechnology = internalTech ? CcdConstants.OutputTechnologyInternal : 5u;
        item.TargetInfo.Id = id;
        item.TargetInfo.AdapterId = new Luid { LowPart = 1, HighPart = 0 };
        item.SourceInfo.Id = id;
        item.SourceInfo.AdapterId = item.TargetInfo.AdapterId;
        item.SourceInfo.ModeInfoIdx = 0x0001FFFF;
        item.TargetInfo.ModeInfoIdx = 0x00020003;
        return item;
    }

    private static PathRow Row(PathRole role, uint id, string name, string monitor) =>
        new(
            (int)id,
            true,
            CcdConstants.DisplayConfigPathActive,
            id,
            id,
            "0000000000000001",
            role == PathRole.Internal ? CcdConstants.OutputTechnologyInternal : 5,
            role == PathRole.Internal,
            @"\\.\DISPLAY" + id,
            @"PCI\VEN_8086",
            name,
            monitor,
            false,
            role);

    private sealed class StubCcd : ICcdApi
    {
        public DisplayConfigPathInfo[] Paths { get; set; } = [];
        public DisplayConfigModeInfo[] Modes { get; set; } = [];
        public List<PathRow> Rows { get; set; } = [];

        public (DisplayConfigPathInfo[] Paths, DisplayConfigModeInfo[] Modes) QueryRaw(uint flags = CcdConstants.QueryFlags) =>
            (Paths, Modes);

        public CcdFrame Capture(uint flags = CcdConstants.QueryFlags) =>
            new(Paths, Modes, new DisplaySnapshot(Rows));

        public DisplaySnapshot QuerySnapshot(uint flags = CcdConstants.QueryFlags) => Capture(flags).Snapshot;

        public int Set(DisplayConfigPathInfo[] paths, DisplayConfigModeInfo[] modes, uint flags)
        {
            if ((flags & CcdConstants.SdcSaveToDatabase) != 0)
            {
                throw new InvalidOperationException("SAVE_TO_DATABASE");
            }

            return 0;
        }

        public int SetTopology(uint topologyFlags) => 0;

        public string? GetLastErrorMessage(int code) => code.ToString();
    }
}
