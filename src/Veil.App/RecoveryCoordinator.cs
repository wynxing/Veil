using System.Diagnostics;
using System.IO;
using Veil.Engine;
using Veil.Engine.Native;
using Veil.Engine.Runtime;
using Veil.Engine.Session;
using Veil.Engine.Topology;

namespace Veil.App;

public sealed class RecoveryCoordinator
{
    private readonly ICcdApi _ccd;
    private readonly Func<string, string?, int> _startRecovery;
    private readonly Func<string, int> _runDriverHelper;
    private string? _directory;
    private int _recoveryPid;
    private IntentFile _intent = new();

    public RecoveryCoordinator(
        ICcdApi ccd,
        Func<string, string?, int>? startRecovery = null,
        Func<string, int>? runDriverHelper = null)
    {
        _ccd = ccd;
        _startRecovery = startRecovery ?? StartRecoveryProcess;
        _runDriverHelper = runDriverHelper ?? RunDriverHelperProcess;
    }

    public bool HasSession => _directory is not null && !File.Exists(SessionPaths.Result(_directory));

    public bool IsReady { get; private set; }

    public bool HotkeyRegistered { get; private set; }

    public HeartbeatFile? Heartbeat { get; private set; }

    public IReadOnlyList<ScreenIdentity> Wanted =>
        _intent.KeepOff.Select(x => x.ToIdentity()).ToList();

    public void Poll()
    {
        if (_directory is null)
        {
            return;
        }

        Heartbeat = JsonUtil.TryRead<HeartbeatFile>(SessionPaths.Heartbeat(_directory));
        if (Heartbeat is not null)
        {
            HotkeyRegistered = Heartbeat.HotkeyRegistered;
            IsReady = Heartbeat.Armed || File.Exists(SessionPaths.Ready(_directory));
        }

        if (File.Exists(SessionPaths.Result(_directory)))
        {
            var result = JsonUtil.TryRead<ResultFile>(SessionPaths.Result(_directory));
            Heartbeat ??= new HeartbeatFile { Detail = result?.Reason };
            IsReady = false;
            HotkeyRegistered = false;
            _directory = null;
            _recoveryPid = 0;
            _intent = new IntentFile();
        }
    }

    public string? KeepOff(ScreenIdentity identity)
    {
        var selected = Wanted.Concat([identity]).DistinctBy(x => (x.AdapterLuid, x.TargetId, x.MonitorPath)).ToList();
        return ApplyIntent(selected);
    }

    public string? RestoreOne(ScreenIdentity identity)
    {
        var selected = Wanted.Where(id => !id.Matches(identity)).ToList();
        return ApplyIntent(selected);
    }

    public string? RestoreAll()
    {
        if (_directory is null)
        {
            return null;
        }

        JsonUtil.WriteAtomic(SessionPaths.Release(_directory), new ReleaseFile { At = DateTimeOffset.UtcNow.ToUnixTimeSeconds() });
        return null;
    }

    public bool RestoreAllAndWait(TimeSpan timeout, out string message)
    {
        if (_directory is null)
        {
            message = "";
            return true;
        }

        RestoreAll();
        var deadline = DateTime.UtcNow + timeout;
        while (DateTime.UtcNow < deadline)
        {
            Poll();
            if (_directory is null)
            {
                message = "";
                return true;
            }

            Thread.Sleep(150);
        }

        message = "恢复超时或失败，未退出。请检查画面，必要时 Win+Ctrl+Shift+B。";
        return false;
    }

    private string? ApplyIntent(List<ScreenIdentity> selected)
    {
        var snapshot = _ccd.QuerySnapshot();
        if (selected.Count == 0)
        {
            return RestoreAll();
        }

        var plan = Gate.PlanKeepOff(snapshot, selected, bundledVddInstalled: DriverStatus.Installed);
        if (plan.Action == KeepOffAction.Blocked)
        {
            return plan.BlockReason;
        }

        if (plan.Action == KeepOffAction.EnableBundledVdd)
        {
            var helperRc = _runDriverHelper("enable");
            if (helperRc != 0)
            {
                return "自带 VDD 未能启用，物理屏未改动。";
            }

            var waitUntil = DateTime.UtcNow.AddSeconds(15);
            while (DateTime.UtcNow < waitUntil)
            {
                snapshot = _ccd.QuerySnapshot();
                if (snapshot.HasActiveBundledVdd)
                {
                    break;
                }

                Thread.Sleep(400);
            }

            if (!snapshot.HasActiveBundledVdd)
            {
                _ = _runDriverHelper("disable");
                return "自带 VDD 未能出现活动虚拟路径，物理屏未改动。";
            }

            plan = Gate.PlanKeepOff(snapshot, selected, bundledVddInstalled: true);
            if (plan.Action != KeepOffAction.Deactivate)
            {
                return plan.BlockReason ?? "启用自带 VDD 后仍无法保持关闭。";
            }
        }

        var planned = DisplayPlanner.ValidateDeactivate(_ccd, selected, plan.AdjustOrigin);
        var validateOk = planned.Ok || (plan.MayAdjustClone && planned.Rc == 87);
        if (!validateOk)
        {
            if (planned.RemainingActive == 0 && planned.Rc == 0)
            {
                return "没有第二活动目标，未 APPLY。";
            }

            return $"无法保持关闭：校验 {planned.Rc}。";
        }

        var error = EnsureRecovery();
        if (error is not null)
        {
            return error;
        }

        _intent = new IntentFile
        {
            KeepOff = selected.Select(ScreenIdentityDto.From).ToList(),
            VddAssist = plan.MayAdjustClone || snapshot.HasActiveBundledVdd,
        };
        JsonUtil.WriteAtomic(SessionPaths.Intent(_directory!), _intent);
        return null;
    }

    private string? EnsureRecovery()
    {
        Poll();
        if (HasSession && IsReady)
        {
            return HotkeyRegistered ? null : "紧急热键不可用。";
        }

        var dir = SessionPaths.NewSessionDirectory();
        var frame = _ccd.Capture();
        TopologyBlob.Save(SessionPaths.Topology(dir), frame.Paths, frame.Modes);
        _recoveryPid = _startRecovery(dir, Environment.ProcessId.ToString());
        var deadline = DateTime.UtcNow.AddSeconds(8);
        ReadyFile? ready = null;
        while (DateTime.UtcNow < deadline)
        {
            ready = JsonUtil.TryRead<ReadyFile>(SessionPaths.Ready(dir));
            if (ready is not null)
            {
                break;
            }

            Thread.Sleep(50);
        }

        if (ready is null || ready.Pid != _recoveryPid || !ready.HotkeyRegistered)
        {
            _directory = null;
            return ready?.HotkeyRegistered == false
                ? "紧急热键不可用。"
                : "恢复进程未就绪。";
        }

        JsonUtil.WriteAtomic(SessionPaths.Arm(dir), new ArmFile { Pid = ready.Pid });
        _directory = dir;
        IsReady = true;
        HotkeyRegistered = true;
        return null;
    }

    private static int StartRecoveryProcess(string directory, string? parentPid)
    {
        var exe = ProcessLaunch.RecoveryExePath();
        var args = $"--directory \"{directory}\" --parent-pid {parentPid}";
        return ProcessLaunch.StartDetached(exe, args);
    }

    private static int RunDriverHelperProcess(string verb)
    {
        var exe = ProcessLaunch.DriverHelperExePath();
        var psi = new ProcessStartInfo(exe, verb)
        {
            UseShellExecute = true,
            Verb = "runas",
        };
        using var proc = Process.Start(psi);
        if (proc is null)
        {
            return 1;
        }

        proc.WaitForExit(60000);
        return proc.HasExited ? proc.ExitCode : 1;
    }
}

public static class DriverStatus
{
    public static bool Installed => File.Exists(Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles),
        "Veil", "vdd", "MttVDD.inf"));
}
