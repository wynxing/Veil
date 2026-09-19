using Veil.Engine.Native;
using Veil.Engine.Runtime;
using Veil.Engine.Session;
using Veil.Engine.Topology;

namespace Veil.Engine.Tests;

public sealed class RecoverySessionTests
{
    [Fact]
    public void HotkeyFailureNeverWritesReadyOrApplies()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        SaveTopology(dir.Path, ccd);
        var hotkey = new FakeHotkey { RegisterSuccess = false };
        var session = new RecoverySession(Options(dir.Path, ccd, hotkey));
        session.Start();
        Assert.True(session.Exited);
        Assert.False(File.Exists(SessionPaths.Ready(dir.Path)));
        Assert.False(ccd.Applied);
        Assert.Equal("error", session.Result.Reason);
    }

    [Fact]
    public void ReleaseBeforeArmExitsWithoutApply()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        SaveTopology(dir.Path, ccd);
        var session = new RecoverySession(Options(dir.Path, ccd, new FakeHotkey(), selfPid: 11));
        session.Start();
        JsonUtil.WriteAtomic(SessionPaths.Release(dir.Path), new ReleaseFile { At = 1 });
        session.Tick();
        Assert.True(session.Exited);
        Assert.False(ccd.Applied);
        Assert.Equal("release", session.Result.Reason);
        Assert.False(session.Result.Ok);
        Assert.Contains("未改物理屏", session.Result.Error);
        Assert.True(File.Exists(SessionPaths.Result(dir.Path)));
    }

    [Fact]
    public void StartExceptionWritesResult()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        SaveTopology(dir.Path, ccd);
        ccd.CaptureException = new InvalidOperationException("ccd-start");
        var session = new RecoverySession(Options(dir.Path, ccd, new FakeHotkey(), selfPid: 11));
        session.Start();
        Assert.True(session.Exited);
        Assert.False(ccd.Applied);
        Assert.Equal("error", session.Result.Reason);
        Assert.Equal("ccd-start", session.Result.Error);
        Assert.True(File.Exists(SessionPaths.Result(dir.Path)));
    }

    [Fact]
    public void TickExceptionWritesResult()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        SaveTopology(dir.Path, ccd);
        var parent = new FakeParent();
        var session = new RecoverySession(Options(dir.Path, ccd, new FakeHotkey(), selfPid: 11, parent: parent));
        session.Start();
        JsonUtil.WriteAtomic(SessionPaths.Arm(dir.Path), new ArmFile { Pid = 11 });
        session.Tick();
        parent.AliveException = new InvalidOperationException("parent-fault");
        session.Tick();
        Assert.True(session.Exited);
        Assert.Equal("error", session.Result.Reason);
        Assert.Equal("parent-fault", session.Result.Error);
        Assert.True(File.Exists(SessionPaths.Result(dir.Path)));
    }

    [Fact]
    public void ArmPidMismatchNeverApplies()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        SaveTopology(dir.Path, ccd);
        var session = new RecoverySession(Options(dir.Path, ccd, new FakeHotkey(), selfPid: 11));
        session.Start();
        JsonUtil.WriteAtomic(SessionPaths.Arm(dir.Path), new ArmFile { Pid = 99 });
        session.Tick();
        Assert.True(session.Exited);
        Assert.False(ccd.Applied);
        Assert.Contains("arm PID", session.Result.Error);
    }

    [Fact]
    public void ApplyHappensOnlyAfterMatchingArmAndIntent()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        SaveTopology(dir.Path, ccd);
        var session = new RecoverySession(Options(dir.Path, ccd, new FakeHotkey(), selfPid: 11));
        session.Start();
        Assert.True(File.Exists(SessionPaths.Ready(dir.Path)));
        Assert.False(ccd.Applied);
        JsonUtil.WriteAtomic(SessionPaths.Arm(dir.Path), new ArmFile { Pid = 11 });
        session.Tick();
        Assert.False(ccd.Applied);
        WriteKeepInternalOff(dir.Path);
        session.Tick();
        Assert.True(ccd.Validated);
        Assert.True(ccd.Applied);
        Assert.Contains(ccd.Flags, f => f == CcdConstants.ApplyFlags);
        Assert.DoesNotContain(ccd.Flags, f => (f & CcdConstants.SdcSaveToDatabase) != 0);
        var heartbeat = JsonUtil.Read<HeartbeatFile>(SessionPaths.Heartbeat(dir.Path));
        Assert.Contains(heartbeat.Screens, s => s.Confirmed == "已关闭");
        Assert.DoesNotContain(heartbeat.Screens, s => s.Wanted == "保持关闭" && s.Confirmed == "已关闭" && s.Detail.Contains("失败"));
    }

    [Fact]
    public void FailedValidateDoesNotApplyAndShowsFailureNotClosed()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        ccd.ValidateRc = 87;
        SaveTopology(dir.Path, ccd);
        var session = ArmWithIntent(dir.Path, ccd);
        Assert.False(ccd.Applied);
        var heartbeat = JsonUtil.Read<HeartbeatFile>(SessionPaths.Heartbeat(dir.Path));
        Assert.Contains(heartbeat.Screens, s => s.Confirmed == "失败");
        Assert.DoesNotContain(heartbeat.Screens, s => s.Confirmed == "已关闭");
        Assert.False(session.Exited);
    }

    [Fact]
    public void ApplyFailureRestores()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        ccd.NextApplyRc = 31;
        SaveTopology(dir.Path, ccd);
        var session = ArmWithIntent(dir.Path, ccd);
        Assert.True(ccd.Applied);
        Assert.Equal(0, session.Result.RestoreRc);
        Assert.False(session.Exited);
    }

    [Fact]
    public void ParentExitRestoresAndWritesResult()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        SaveTopology(dir.Path, ccd);
        var parent = new FakeParent();
        var session = ArmWithIntent(dir.Path, ccd, parent: parent);
        parent.Alive = false;
        session.Tick();
        Assert.True(session.Exited);
        Assert.Equal("parent-exit", session.Result.Reason);
        Assert.Equal(0, session.Result.RestoreRc);
    }

    [Fact]
    public void TopologyChurnWhilePhysicalStillOffDoesNotBurnReapply()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        SaveTopology(dir.Path, ccd);
        var session = ArmWithIntent(dir.Path, ccd);
        Assert.False(session.Exited);
        var remaining = ccd.Paths[1];
        remaining.TargetInfo.Id = 99;
        ccd.Paths[1] = remaining;
        ccd.Rows[1] = ccd.Rows[1] with { TargetId = 99 };
        session.Tick();
        Assert.False(session.Exited);
        Assert.False(session.Result.ReapplyAttempted);
        var events = File.ReadAllText(SessionPaths.Events(dir.Path));
        Assert.Contains("topology-settle", events);
        Assert.DoesNotContain("\"type\":\"interrupt\"", events);
    }

    [Fact]
    public void ExecutionGapRestoresThenReappliesOnce()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        SaveTopology(dir.Path, ccd);
        var clock = new FakeClock { Seconds = 0 };
        var session = ArmWithIntent(dir.Path, ccd, clock: clock);
        var applyCount = ccd.Flags.Count(f => f == CcdConstants.ApplyFlags);
        clock.Seconds = 10;
        session.Tick();
        Assert.True(session.Result.ReapplyAttempted);
        Assert.True(ccd.Flags.Count(f => f == CcdConstants.ApplyFlags) > applyCount);
        Assert.False(session.Exited);
        clock.Seconds = 20;
        session.Tick();
        Assert.True(session.Exited);
        Assert.Equal("execution-gap", session.Result.Reason);
    }

    [Fact]
    public void StaleCaptureAfterRestoreStillReapplies()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        SaveTopology(dir.Path, ccd);
        var clock = new FakeClock { Seconds = 0 };
        var session = ArmWithIntent(dir.Path, ccd, clock: clock);
        var applyCount = ccd.Flags.Count(f => f == CcdConstants.ApplyFlags);
        ccd.StaleCapturesRemaining = 2;
        clock.Seconds = 10;
        session.Tick();
        Assert.True(session.Result.ReapplyAttempted);
        Assert.True(ccd.Flags.Count(f => f == CcdConstants.ApplyFlags) > applyCount);
        Assert.False(session.Exited);
        var events = File.ReadAllText(SessionPaths.Events(dir.Path));
        Assert.Contains("reapply-settle", events);
        Assert.Contains("reapplied", events);
        Assert.DoesNotContain("already-off", events);
    }

    [Fact]
    public void ReapplyWithoutSecondTargetRequestsBundledVdd()
    {
        using var dir = new TempSession();
        var ccd = InternalPlusBundledVdd();
        SaveTopology(dir.Path, ccd);
        var clock = new FakeClock { Seconds = 0 };
        var session = ArmWithIntent(dir.Path, ccd, clock: clock, vddAssist: true);
        ccd.AfterApply = () =>
        {
            ccd.Paths = [ccd.Paths[0]];
            ccd.Rows = [ccd.Rows[0] with { Active = true }];
        };
        clock.Seconds = 10;
        session.Tick();
        Assert.True(session.Result.ReapplyAttempted);
        Assert.False(session.Exited);
        Assert.True(File.Exists(SessionPaths.VddRequest(dir.Path)));
        var events = File.ReadAllText(SessionPaths.Events(dir.Path));
        Assert.Contains("vdd-request", events);
        ccd.AfterApply = null;
        ccd.Paths = InternalPlusBundledVdd().Paths;
        ccd.Rows = InternalPlusBundledVdd().Rows;
        session.Tick();
        Assert.False(session.Exited);
        Assert.Contains("reapplied", File.ReadAllText(SessionPaths.Events(dir.Path)));
    }

    [Fact]
    public void DualPhysicalNeverUsesCloneFlags()
    {
        using var dir = new TempSession();
        var ccd = DualPhysicalCcd();
        SaveTopology(dir.Path, ccd);
        ArmWithIntent(dir.Path, ccd);
        Assert.DoesNotContain(ccd.Flags, f => (f & CcdConstants.SdcTopologyClone) != 0);
    }

    private static RecoverySession ArmWithIntent(string dir, FakeCcd ccd, FakeParent? parent = null, FakeClock? clock = null, bool vddAssist = false)
    {
        var session = new RecoverySession(Options(dir, ccd, new FakeHotkey(), selfPid: 11, parent: parent, clock: clock));
        session.Start();
        JsonUtil.WriteAtomic(SessionPaths.Arm(dir), new ArmFile { Pid = 11 });
        session.Tick();
        WriteKeepInternalOff(dir, vddAssist);
        session.Tick();
        return session;
    }

    private static RecoveryOptions Options(
        string dir,
        ICcdApi ccd,
        IHotkey hotkey,
        int selfPid = 11,
        FakeParent? parent = null,
        FakeClock? clock = null) =>
        new()
        {
            Directory = dir,
            SelfPid = selfPid,
            ParentPid = 22,
            Ccd = ccd,
            Hotkey = hotkey,
            Parent = parent ?? new FakeParent(),
            Clock = clock ?? new FakeClock(),
            ArmTimeoutSeconds = 10,
            GapSeconds = 3,
            ReapplySettleAttempts = 4,
            Pause = _ => { },
        };

    private static FakeCcd InternalPlusBundledVdd()
    {
        var internalPath = PathFactory.Path(id: 1);
        var vddPath = PathFactory.Path(internalTech: false, id: 2);
        return new FakeCcd
        {
            Paths = [internalPath, vddPath],
            Modes = [new DisplayConfigModeInfo { InfoType = 1 }],
            Rows =
            [
                PathFactory.Row(PathRole.Internal, targetId: 1, monitorPath: @"\\?\DISPLAY#CMN#1"),
                PathFactory.Row(
                    PathRole.Virtual,
                    targetId: 2,
                    name: "VDD by MTT",
                    adapterPath: @"ROOT\MttVDD\0000",
                    monitorPath: @"\\?\DISPLAY#MTT1337#1"),
            ],
        };
    }

    private static FakeCcd DualPhysicalCcd()
    {
        var internalPath = PathFactory.Path(id: 1);
        var externalPath = PathFactory.Path(internalTech: false, id: 2);
        return new FakeCcd
        {
            Paths = [internalPath, externalPath],
            Modes = [new DisplayConfigModeInfo { InfoType = 1 }],
            Rows =
            [
                PathFactory.Row(PathRole.Internal, targetId: 1, monitorPath: @"\\?\DISPLAY#CMN#1"),
                PathFactory.Row(PathRole.External, targetId: 2, name: "S24", monitorPath: @"\\?\DISPLAY#PDA#1"),
            ],
        };
    }

    private static void SaveTopology(string dir, FakeCcd ccd) =>
        TopologyBlob.Save(SessionPaths.Topology(dir), ccd.Paths, ccd.Modes);

    private static void WriteKeepInternalOff(string dir, bool vddAssist = false) =>
        JsonUtil.WriteAtomic(SessionPaths.Intent(dir), new IntentFile
        {
            KeepOff = [new ScreenIdentityDto { AdapterLuid = "0000000000000001", TargetId = 1, MonitorPath = @"\\?\DISPLAY#CMN#1" }],
            VddAssist = vddAssist,
        });

    private sealed class TempSession : IDisposable
    {
        public string Path { get; } = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "veil-test-" + Guid.NewGuid().ToString("N"));

        public TempSession() => Directory.CreateDirectory(Path);

        public void Dispose()
        {
            try
            {
                Directory.Delete(Path, true);
            }
            catch (IOException)
            {
            }
        }
    }
}
