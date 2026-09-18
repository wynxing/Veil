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
        WriteHeartbeat("等待 arm。");
        _started = _opt.Clock.Seconds;
        _previous = _started;
    }

    public void Tick()
    {
        if (Exited)
        {
            return;
        }

        var now = _opt.Clock.Seconds;
        var hotkey = _opt.Hotkey.WasPressed();
        if (!_armed)
        {
            if (hotkey)
            {
                Fail("cancelled-before-arm", null, restore: false);
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
        if (plan.Action == KeepOffAction.Blocked || plan.Action == KeepOffAction.EnableBundledVdd)
        {
            WriteHeartbeat(plan.BlockReason ?? Gate.LastPathReason, selected, failed: true);
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
                WriteHeartbeat("已保持关闭。", selected, failed: false);
                return true;
            }

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
        WriteHeartbeat(isReapply ? "已再次保持关闭。" : "已保持关闭。", selected, failed: false);
        return true;
    }

    private void HandleInterrupt(string reason)
    {
        Result.Reason = reason;
        RestoreSaved();
        var selected = _intent.KeepOff.Select(x => x.ToIdentity()).ToList();
        if (!_reapplyAttempted && selected.Count > 0)
        {
            _reapplyAttempted = true;
            Result.ReapplyAttempted = true;
            WriteHeartbeat("会话中断，尝试再关一次。", selected);
            if (TryApply(selected, isReapply: true) && _holding)
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

        JsonUtil.WriteAtomic(SessionPaths.Heartbeat(_opt.Directory), new HeartbeatFile
        {
            HotkeyRegistered = _hotkeyRegistered,
            Armed = _armed,
            Screens = screens,
            Detail = detail,
        });
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

    private static bool TargetsEqual(List<(string Adapter, uint TargetId)> a, List<(string Adapter, uint TargetId)> b) =>
        a.Count == b.Count && a.Zip(b).All(pair => pair.First == pair.Second);
}
