using System.Text.Json;
using System.Text.Json.Serialization;

namespace Veil.Engine.Session;

public static class JsonUtil
{
    public static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = true,
        PropertyNameCaseInsensitive = true,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    public static void WriteAtomic<T>(string path, T value)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path)) ?? ".");
        var tmp = path + ".tmp";
        var payload = JsonSerializer.Serialize(value, Options);
        for (var attempt = 0; ; attempt++)
        {
            try
            {
                File.WriteAllText(tmp, payload);
                File.Move(tmp, path, overwrite: true);
                return;
            }
            catch (IOException) when (attempt < 7)
            {
                Thread.Sleep(20);
            }
        }
    }

    public static T Read<T>(string path) =>
        JsonSerializer.Deserialize<T>(File.ReadAllText(path), Options)
        ?? throw new InvalidOperationException($"invalid json: {path}");

    public static T? TryRead<T>(string path) where T : class
    {
        try
        {
            return File.Exists(path) ? Read<T>(path) : null;
        }
        catch (IOException)
        {
            return null;
        }
        catch (JsonException)
        {
            return null;
        }
    }
}

public sealed class ReadyFile
{
    [JsonPropertyName("pid")]
    public int Pid { get; set; }

    [JsonPropertyName("hotkeyRegistered")]
    public bool HotkeyRegistered { get; set; }

    [JsonPropertyName("hotkey")]
    public string Hotkey { get; set; } = Native.CcdConstants.HotkeyText;
}

public sealed class ArmFile
{
    [JsonPropertyName("pid")]
    public int Pid { get; set; }
}

public sealed class IntentFile
{
    [JsonPropertyName("keepOff")]
    public List<ScreenIdentityDto> KeepOff { get; set; } = [];

    [JsonPropertyName("vddAssist")]
    public bool VddAssist { get; set; }
}

public sealed class ScreenIdentityDto
{
    [JsonPropertyName("adapterLuid")]
    public string AdapterLuid { get; set; } = "";

    [JsonPropertyName("targetId")]
    public uint TargetId { get; set; }

    [JsonPropertyName("monitorPath")]
    public string MonitorPath { get; set; } = "";

    public ScreenIdentity ToIdentity() => new(AdapterLuid, TargetId, MonitorPath);

    public static ScreenIdentityDto From(ScreenIdentity id) => new()
    {
        AdapterLuid = id.AdapterLuid,
        TargetId = id.TargetId,
        MonitorPath = id.MonitorPath,
    };
}

public sealed class ReleaseFile
{
    [JsonPropertyName("at")]
    public double At { get; set; }
}

public sealed class HeartbeatScreen
{
    [JsonPropertyName("adapterLuid")]
    public string AdapterLuid { get; set; } = "";

    [JsonPropertyName("targetId")]
    public uint TargetId { get; set; }

    [JsonPropertyName("monitorPath")]
    public string MonitorPath { get; set; } = "";

    [JsonPropertyName("name")]
    public string Name { get; set; } = "";

    [JsonPropertyName("wanted")]
    public string Wanted { get; set; } = "开启";

    [JsonPropertyName("confirmed")]
    public string Confirmed { get; set; } = "未知";

    [JsonPropertyName("detail")]
    public string Detail { get; set; } = "";
}

public sealed class HeartbeatFile
{
    [JsonPropertyName("hotkeyRegistered")]
    public bool HotkeyRegistered { get; set; }

    [JsonPropertyName("armed")]
    public bool Armed { get; set; }

    [JsonPropertyName("screens")]
    public List<HeartbeatScreen> Screens { get; set; } = [];

    [JsonPropertyName("detail")]
    public string? Detail { get; set; }
}

public sealed class ResultFile
{
    [JsonPropertyName("ok")]
    public bool Ok { get; set; }

    [JsonPropertyName("reason")]
    public string Reason { get; set; } = "error";

    [JsonPropertyName("applyRc")]
    public int? ApplyRc { get; set; }

    [JsonPropertyName("restoreRc")]
    public int? RestoreRc { get; set; }

    [JsonPropertyName("fallbackRc")]
    public int? FallbackRc { get; set; }

    [JsonPropertyName("restoredTopology")]
    public bool RestoredTopology { get; set; }

    [JsonPropertyName("restoredTargets")]
    public bool RestoredTargets { get; set; }

    [JsonPropertyName("reapplyAttempted")]
    public bool ReapplyAttempted { get; set; }

    [JsonPropertyName("error")]
    public string? Error { get; set; }

    [JsonPropertyName("adjustedOrigin")]
    public bool? AdjustedOrigin { get; set; }

    [JsonPropertyName("adjustedClone")]
    public bool? AdjustedClone { get; set; }
}

public static class SessionPaths
{
    public static string Root => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Veil");

    public static string NewSessionDirectory()
    {
        var dir = Path.Combine(Root, "session-" + Guid.NewGuid().ToString("N")[..8]);
        Directory.CreateDirectory(dir);
        return dir;
    }

    public static string Topology(string dir) => Path.Combine(dir, "topology.json");
    public static string Ready(string dir) => Path.Combine(dir, "ready.json");
    public static string Arm(string dir) => Path.Combine(dir, "arm.json");
    public static string Intent(string dir) => Path.Combine(dir, "intent.json");
    public static string Release(string dir) => Path.Combine(dir, "release.json");
    public static string Heartbeat(string dir) => Path.Combine(dir, "heartbeat.json");
    public static string Result(string dir) => Path.Combine(dir, "result.json");
    public static string Events(string dir) => Path.Combine(dir, "events.jsonl");
    public static string VddRequest(string dir) => Path.Combine(dir, "vdd-request.json");
}

public sealed class VddRequestFile
{
    [JsonPropertyName("at")]
    public double At { get; set; }

    [JsonPropertyName("reason")]
    public string Reason { get; set; } = "reapply";
}
