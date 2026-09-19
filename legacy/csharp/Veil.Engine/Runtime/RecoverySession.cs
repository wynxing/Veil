using Veil.Engine.Native;
using Veil.Engine.Session;
using Veil.Engine.Topology;

namespace Veil.Engine.Runtime;

public sealed class RecoveryOptions
{
    public required string Directory { get; init; }
    public int SelfPid { get; init; } = Environment.ProcessId;
    public int ParentPid { get; init; }
    public required ICcdApi Ccd { get; init; }
    public required IHotkey Hotkey { get; init; }
    public IMonotonicClock Clock { get; init; } = new SystemMonotonicClock();
    public IParentWatcher Parent { get; init; } = new Win32ParentWatcher();
    public double ArmTimeoutSeconds { get; init; } = 10;
    public double GapSeconds { get; init; } = 3;
    public int ReapplySettleAttempts { get; init; } = 12;
    public TimeSpan ReapplySettlePause { get; init; } = TimeSpan.FromMilliseconds(150);
    public Action<TimeSpan> Pause { get; init; } = Thread.Sleep;
    public double VddWaitSeconds { get; init; } = 20;
}

public sealed class RecoverySession
{
    private readonly RecoveryOptions _opt;
    private DisplayConfigPathInfo[] _savedPaths = [];
    private DisplayConfigModeInfo[] _savedModes = [];
    private string _savedFingerprint = "";
    private bool _armed;
    private bool _holding;
    private bool _reapplyAttempted;
    private bool _waitingVdd;
    private double _vddWaitStart;
    private bool _hotkeyRegistered;
    private double _started;
    private double _previous;
    private string? _lastIntentText;
    private List<(string Adapter, uint TargetId)>? _expectedTargets;
    private IntentFile _intent = new();

    public RecoverySession(RecoveryOptions options)
    {
        _opt = options;
        Result = new ResultFile { Reason = "not-armed", Ok = false };
    }

    public bool Exited { get; private set; }
    public ResultFile Result { get; }

    public void Start()
    {
        try
        {
            CcdAbi.EnsureExpectedLayout();
            var topologyPath = SessionPaths.Topology(_opt.Directory);
            if (!File.Exists(topologyPath))
            {
                Fail("error", "missing topology.json", restore: false);
                return;
            }

            (_savedPaths, _savedModes) = TopologyBlob.Load(topologyPath);
            _savedFingerprint = TopologyBlob.Fingerprint(_savedPaths, _savedModes);
            var current = _opt.Ccd.Capture();
            if (TopologyBlob.Fingerprint(current.Paths, current.Modes) != _savedFingerprint)
            {
                Fail("error", "topology changed since save", restore: false);
                return;
            }

            if (!_opt.Hotkey.TryRegister())
            {
                Fail("error", "RegisterHotKey failed", restore: false);
                return;
            }

            _hotkeyRegistered = true;
            JsonUtil.WriteAtomic(SessionPaths.Ready(_opt.Directory), new ReadyFile
            {
                Pid = _opt.SelfPid,
                HotkeyRegistered = true,
                Hotkey = CcdConstants.HotkeyText,
            });
            SessionLog.Append(_opt.Directory, "ready", detail: "热键已注册。");
            WriteHeartbeat("等待 arm。");
            _started = _opt.Clock.Seconds;
            _previous = _started;
        }
        catch (Exception ex)
        {
            Fail("error", ex.Message, restore: false);
        }
    }

    public void Tick()
    {
        if (Exited)
        {
            return;
        }

        try
        {
            TickCore();
        }
        catch (Exception ex)
        {
            Fail("error", ex.Message, restore: _holding || _armed);
        }
    }

    private void TickCore()
    {
        var now = _opt.Clock.Seconds;
        var hotkey = _opt.Hotkey.WasPressed();
        if (!_armed)
        {
            if (hotkey)
            {
                Fail("cancelled-before-arm", null, restore: false);
                return;
            }

            if (File.Exists(SessionPaths.Release(_opt.Directory)))
            {
                Fail("release", "已取消，未改物理屏。", restore: false);
                return;
            }

            var arm = JsonUtil.TryRead<ArmFile>(SessionPaths.Arm(_opt.Directory));
            if (arm is not null)
            {
                if (arm.Pid != _opt.SelfPid)
                {
                    Fail("error", "arm PID mismatch", restore: false);
                    return;
                }

                _armed = true;
                _previous = now;
                SessionLog.Append(_opt.Directory, "armed");
                WriteHeartbeat("已 arm。");
                return;
            }

            if (now - _started >= _opt.ArmTimeoutSeconds)
            {
                Fail("not-armed", "recovery worker not ready within timeout", restore: false);
            }

            return;
        }

        if (hotkey)
        {
            Finish("hotkey", restore: true);
            return;
        }

        if (File.Exists(SessionPaths.Release(_opt.Directory)))
        {
            Finish("release", restore: true);
            return;
        }

        if (_opt.ParentPid > 0 && !_opt.Parent.IsAlive(_opt.ParentPid))
        {
            Finish("parent-exit", restore: true);
            return;
        }

        if (_waitingVdd)
        {
            TickWaitingVdd(now);
            return;
        }

        if (now - _previous > _opt.GapSeconds)
        {
            HandleInterrupt("execution-gap");
            return;
        }

        _previous = now;
        ApplyIntentIfNeeded();
        if (Exited)
        {
            return;
        }

        if (_holding && _expectedTargets is not null)
        {
            var frame = _opt.Ccd.Capture();
            var current = PathOps.ActiveTargets(frame.Paths);
            if (!TargetsEqual(current, _expectedTargets))
            {
                var selected = _intent.KeepOff.Select(x => x.ToIdentity()).ToList();
                if (KeepOffStillHolds(frame.Snapshot, selected))
                {
                    _expectedTargets = current;
                    SessionLog.Append(_opt.Directory, "topology-settle", detail: "活动目标变了，所选物理屏仍关。");
                    WriteHeartbeat("拓扑微调，仍保持关闭。", selected);
                    return;
                }

                HandleInterrupt("unexpected-topology");
            }
        }
    }

    public void RunUntilExit(TimeSpan? slice = null)
    {
        var pause = slice ?? TimeSpan.FromMilliseconds(50);
        Start();
        while (!Exited)
        {
            Tick();
            if (!Exited)
            {
                Thread.Sleep(pause);
            }
        }
    }

    private void ApplyIntentIfNeeded()
    {
        var path = SessionPaths.Intent(_opt.Directory);
        if (!File.Exists(path))
        {
            return;
        }

        string text;
        try
        {
            text = File.ReadAllText(path);
        }
        catch (IOException)
        {
            return;
        }

        if (text == _lastIntentText)
        {
            return;
        }

        IntentFile? intent;
        try
        {
            intent = JsonUtil.Read<IntentFile>(path);
        }
        catch (Exception ex)
        {
            WriteHeartbeat($"intent 无效：{ex.Message}");
            return;
        }

        _lastIntentText = text;
        _intent = intent;
        var selected = intent.KeepOff.Select(x => x.ToIdentity()).ToList();
        if (selected.Count == 0)
        {
            Finish("release", restore: true);
            return;
        }

        if (!TryApply(selected, isReapply: false))
        {
            _lastIntentText = null;
        }
    }

    private bool TryApply(IReadOnlyList<ScreenIdentity> selected, bool isReapply)
    {
        var frame = _opt.Ccd.Capture();
        var plan = Gate.PlanKeepOff(frame.Snapshot, selected, bundledVddInstalled: _intent.VddAssist || frame.Snapshot.HasActiveBundledVdd);
        if (plan.Action == KeepOffAction.EnableBundledVdd)
        {
            if (isReapply)
            {
                RequestBundledVdd();
                return false;
            }

            var enable = plan.BlockReason ?? Gate.EnableVddReason;
            SessionLog.Append(_opt.Directory, "apply-blocked", detail: enable, reason: plan.Action.ToString(), reapply: false);
            WriteHeartbeat(enable, selected, failed: true);
            return false;
        }

        if (plan.Action == KeepOffAction.Blocked)
        {
            var blocked = plan.BlockReason ?? Gate.LastPathReason;
            SessionLog.Append(_opt.Directory, "apply-blocked", detail: blocked, reason: plan.Action.ToString(), reapply: isReapply);
            WriteHeartbeat(blocked, selected, failed: true);
            if (isReapply)
            {
                Finish(Result.Reason, restore: true);
            }

            return false;
        }

        var identities = frame.Snapshot.Paths.Select(p => p.Identity).ToList();
        var stillActive = selected.Where(id =>
            frame.Snapshot.Paths.Any(p => p.Active && p.IsPhysical && id.Matches(p.Identity))).ToList();
        if (stillActive.Count == 0)
        {
            if (frame.Snapshot.ActivePaths.Any())
            {
                _holding = true;
                _expectedTargets = PathOps.ActiveTargets(frame.Paths);
                SessionLog.Append(_opt.Directory, "already-off", detail: "所选物理屏已关，未再 APPLY。", reapply: isReapply);
                WriteHeartbeat(isReapply ? "醒后所选屏仍关着。" : "已保持关闭。", selected, failed: false);
                return true;
            }

            SessionLog.Append(_opt.Directory, "apply-blocked", detail: "没有剩余活动路径。", reapply: isReapply);
            WriteHeartbeat("VALIDATE 后没有剩余活动路径，未 APPLY。", selected, failed: true);
            return false;
        }

        var prepared = PathOps.Deactivate(frame.Paths, frame.Modes, identities, stillActive, plan.AdjustOrigin);
        if (!prepared.CanApply)
        {
            WriteHeartbeat("VALIDATE 后没有剩余活动路径，未 APPLY。", selected, failed: true);
            return false;
        }

        var paths = prepared.Paths;
        var modes = prepared.Modes;
        var adjustedClone = false;
        var rc = _opt.Ccd.Set(paths, modes, CcdConstants.ValidateFlags);
        if (rc == 87 && plan.MayAdjustClone)
        {
            var cloneRc = _opt.Ccd.SetTopology(CcdConstants.SdcApply | CcdConstants.SdcTopologyClone);
            if (cloneRc != 0)
            {
                WriteHeartbeat($"无法改为共用源拓扑：{cloneRc}。", selected, failed: true);
                RestoreSaved();
                return false;
            }

            adjustedClone = true;
            frame = _opt.Ccd.Capture();
            identities = frame.Snapshot.Paths.Select(p => p.Identity).ToList();
            stillActive = selected.Where(id =>
                frame.Snapshot.Paths.Any(p => p.Active && p.IsPhysical && id.Matches(p.Identity))).ToList();
            prepared = PathOps.Deactivate(frame.Paths, frame.Modes, identities, stillActive, plan.AdjustOrigin);
            if (!prepared.CanApply)
            {
                WriteHeartbeat("改为共用源后仍无法留下活动路径。", selected, failed: true);
                RestoreSaved();
                return false;
            }

            paths = prepared.Paths;
            modes = prepared.Modes;
            rc = _opt.Ccd.Set(paths, modes, CcdConstants.ValidateFlags);
            if (rc != 0)
            {
                WriteHeartbeat($"无法保持关闭：校验 {rc}。", selected, failed: true);
                RestoreSaved();
                return false;
            }
        }

        if (rc != 0)
        {
            SessionLog.Append(_opt.Directory, "apply-blocked", detail: $"VALIDATE {rc}", reapply: isReapply, applyRc: rc);
            WriteHeartbeat($"无法保持关闭：校验 {rc}。", selected, failed: true);
            Result.ApplyRc = rc;
            return false;
        }

        var applyRc = _opt.Ccd.Set(paths, modes, CcdConstants.ApplyFlags);
        Result.ApplyRc = applyRc;
        Result.AdjustedOrigin = prepared.AdjustedOrigin;
        Result.AdjustedClone = adjustedClone;
        if (applyRc != 0)
        {
            SessionLog.Append(_opt.Directory, "apply-failed", detail: $"APPLY {applyRc}", reapply: isReapply, applyRc: applyRc);
            WriteHeartbeat($"APPLY 失败：{applyRc}。", selected, failed: true);
            RestoreSaved();
            if (isReapply)
            {
                Finish("error", restore: false);
            }

            return false;
        }

        _holding = true;
        _expectedTargets = PathOps.ActiveTargets(paths);
        SessionLog.Append(
            _opt.Directory,
            isReapply ? "reapplied" : "applied",
            detail: isReapply ? "已再次保持关闭。" : "已保持关闭。",
            reapply: isReapply,
            applyRc: applyRc);
        WriteHeartbeat(isReapply ? "已再次保持关闭。" : "已保持关闭。", selected, failed: false);
        return true;
    }

    private void HandleInterrupt(string reason)
    {
        Result.Reason = reason;
        SessionLog.Append(_opt.Directory, "interrupt", reason: reason, reapply: _reapplyAttempted);
        RestoreSaved();
        var selected = _intent.KeepOff.Select(x => x.ToIdentity()).ToList();
        if (!_reapplyAttempted && selected.Count > 0)
        {
            _reapplyAttempted = true;
            Result.ReapplyAttempted = true;
            SessionLog.Append(_opt.Directory, "reapply-attempt", reason: reason);
            WriteHeartbeat("会话中断，尝试再关一次。", selected);
            WaitForSelectedPhysical(selected);
            if (TryApply(selected, isReapply: true) && _holding)
            {
                _previous = _opt.Clock.Seconds;
                return;
            }

            if (_waitingVdd)
            {
                _previous = _opt.Clock.Seconds;
                return;
            }
        }

        Finish(reason, restore: false);
    }

    private void Finish(string reason, bool restore)
    {
        if (Exited)
        {
            return;
        }

        Result.Reason = reason;
        if (restore)
        {
            RestoreSaved();
        }

        Result.Ok = reason is "hotkey" or "release" or "parent-exit"
                    && Result.ApplyRc is 0 or null
                    && Result.RestoreRc is 0
                    && Result.RestoredTargets
                    && Result.RestoredTopology;
        WriteHeartbeat(FinishHeartbeat(reason));
        SessionLog.Append(
            _opt.Directory,
            "finish",
            detail: FinishHeartbeat(reason),
            reason: reason,
            reapply: Result.ReapplyAttempted,
            applyRc: Result.ApplyRc);
        WriteResult();
    }

    private void Fail(string reason, string? error, bool restore)
    {
        Result.Reason = reason;
        Result.Error = error;
        Result.Ok = false;
        if (restore)
        {
            RestoreSaved();
        }

        WriteHeartbeat(error ?? reason);
        SessionLog.Append(_opt.Directory, "finish", detail: error ?? reason, reason: reason);
        WriteResult();
    }

    private void RestoreSaved()
    {
        if (_savedPaths.Length == 0)
        {
            return;
        }

        Result.RestoreRc = _opt.Ccd.Set(_savedPaths, _savedModes, CcdConstants.ApplyFlags);
        var after = _opt.Ccd.Capture();
        Result.RestoredTargets = TargetsEqual(PathOps.ActiveTargets(after.Paths), PathOps.ActiveTargets(_savedPaths));
        Result.RestoredTopology = TopologyBlob.Fingerprint(after.Paths, after.Modes) == _savedFingerprint;
        if (Result.RestoreRc != 0 || !Result.RestoredTargets)
        {
            Result.FallbackRc = _opt.Ccd.SetTopology(CcdConstants.SdcApply | CcdConstants.SdcTopologyInternal);
        }

        _holding = false;
        _expectedTargets = null;
    }

    private void WriteHeartbeat(string detail, IReadOnlyList<ScreenIdentity>? selected = null, bool failed = false)
    {
        selected ??= _intent.KeepOff.Select(x => x.ToIdentity()).ToList();
        DisplaySnapshot snapshot;
        try
        {
            snapshot = _opt.Ccd.Capture().Snapshot;
        }
        catch (Exception)
        {
            snapshot = new DisplaySnapshot([]);
        }

        var screens = new List<HeartbeatScreen>();
        foreach (var row in snapshot.PhysicalScreens)
        {
            var wanted = selected.Any(id => id.Matches(row.Identity)) ? "保持关闭" : "开启";
            string confirmed;
            if (wanted == "保持关闭")
            {
                if (failed)
                {
                    confirmed = "失败";
                }
                else if (!row.Active && _holding)
                {
                    confirmed = "已关闭";
                }
                else if (_armed && !_holding)
                {
                    confirmed = "处理中";
                }
                else
                {
                    confirmed = row.Active ? "处理中" : "已关闭";
                }
            }
            else
            {
                confirmed = row.Active ? "已显示" : "未知";
            }

            screens.Add(new HeartbeatScreen
            {
                AdapterLuid = row.AdapterLuid,
                TargetId = row.TargetId,
                MonitorPath = row.MonitorPath,
                Name = row.DisplayName,
                Wanted = wanted,
                Confirmed = confirmed,
                Detail = wanted == "保持关闭" ? detail : "",
            });
        }

        foreach (var id in selected)
        {
            if (screens.Any(s => id.Matches(new ScreenIdentity(s.AdapterLuid, s.TargetId, s.MonitorPath))))
            {
                continue;
            }

            screens.Add(new HeartbeatScreen
            {
                AdapterLuid = id.AdapterLuid,
                TargetId = id.TargetId,
                MonitorPath = id.MonitorPath,
                Name = id.MonitorPath,
                Wanted = "保持关闭",
                Confirmed = failed ? "失败" : (_holding ? "已关闭" : "处理中"),
                Detail = detail,
            });
        }

        try
        {
            JsonUtil.WriteAtomic(SessionPaths.Heartbeat(_opt.Directory), new HeartbeatFile
            {
                HotkeyRegistered = _hotkeyRegistered,
                Armed = _armed,
                Screens = screens,
                Detail = detail,
            });
        }
        catch (IOException)
        {
        }
    }

    private void WriteResult()
    {
        if (_hotkeyRegistered)
        {
            _opt.Hotkey.Unregister();
            _hotkeyRegistered = false;
        }

        JsonUtil.WriteAtomic(SessionPaths.Result(_opt.Directory), Result);
        Exited = true;
    }

    private void WaitForSelectedPhysical(IReadOnlyList<ScreenIdentity> selected)
    {
        var attempts = Math.Max(1, _opt.ReapplySettleAttempts);
        for (var i = 0; i < attempts; i++)
        {
            var snap = _opt.Ccd.Capture().Snapshot;
            if (selected.Any(id => snap.Paths.Any(p => p.Active && p.IsPhysical && id.Matches(p.Identity))))
            {
                SessionLog.Append(_opt.Directory, "reapply-settle", detail: $"selected-active attempt {i + 1}");
                return;
            }

            if (i + 1 < attempts)
            {
                _opt.Pause(_opt.ReapplySettlePause);
            }
        }

        SessionLog.Append(_opt.Directory, "reapply-settle", detail: "timeout, selected still off");
    }

    private void RequestBundledVdd()
    {
        _waitingVdd = true;
        _vddWaitStart = _opt.Clock.Seconds;
        JsonUtil.WriteAtomic(SessionPaths.VddRequest(_opt.Directory), new VddRequestFile
        {
            At = _vddWaitStart,
            Reason = "reapply",
        });
        SessionLog.Append(_opt.Directory, "vdd-request", detail: "再关需要再次启用自带 VDD。");
        WriteHeartbeat("等待再次启用隐藏辅助输出。");
    }

    private void TickWaitingVdd(double now)
    {
        if (now - _vddWaitStart > _opt.VddWaitSeconds)
        {
            SessionLog.Append(_opt.Directory, "apply-blocked", detail: "等待自带 VDD 超时。");
            Finish(string.IsNullOrEmpty(Result.Reason) || Result.Reason == "not-armed" ? "execution-gap" : Result.Reason, restore: true);
            return;
        }

        var snap = _opt.Ccd.Capture().Snapshot;
        if (!snap.HasActiveBundledVdd)
        {
            return;
        }

        TryDelete(SessionPaths.VddRequest(_opt.Directory));
        SessionLog.Append(_opt.Directory, "vdd-ready");
        var selected = _intent.KeepOff.Select(x => x.ToIdentity()).ToList();
        _waitingVdd = false;
        if (TryApply(selected, isReapply: true) && _holding)
        {
            _previous = now;
            return;
        }

        Finish(Result.Reason, restore: false);
    }

    private static void TryDelete(string path)
    {
        try
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
        catch (IOException)
        {
        }
    }

    private static bool KeepOffStillHolds(DisplaySnapshot snapshot, IReadOnlyList<ScreenIdentity> selected)
    {
        if (!snapshot.ActivePaths.Any())
        {
            return false;
        }

        foreach (var id in selected)
        {
            var row = snapshot.Paths.FirstOrDefault(p => p.IsPhysical && id.Matches(p.Identity));
            if (row is not null && row.Active)
            {
                return false;
            }
        }

        return true;
    }

    private static string FinishHeartbeat(string reason) => reason switch
    {
        "hotkey" => "已由热键恢复。",
        "release" => "已恢复全部。",
        "parent-exit" => "界面退出后已恢复。",
        "execution-gap" => "会话中断，保持关闭已结束。",
        "unexpected-topology" => "显示拓扑已变化，保持关闭已结束。",
        _ => "保持关闭已结束。",
    };

    private static bool TargetsEqual(List<(string Adapter, uint TargetId)> a, List<(string Adapter, uint TargetId)> b) =>
        a.Count == b.Count && a.Zip(b).All(pair => pair.First == pair.Second);
}
