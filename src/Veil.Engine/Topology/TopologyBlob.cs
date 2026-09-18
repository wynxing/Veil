using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;
using Veil.Engine.Native;

namespace Veil.Engine.Topology;

public sealed class TopologyBlob
{
    [JsonPropertyName("version")]
    public int Version { get; set; } = 1;

    [JsonPropertyName("queryFlags")]
    public uint QueryFlags { get; set; } = CcdConstants.QueryFlags;

    [JsonPropertyName("pathCount")]
    public int PathCount { get; set; }

    [JsonPropertyName("modeCount")]
    public int ModeCount { get; set; }

    [JsonPropertyName("pathB64")]
    public string PathB64 { get; set; } = "";

    [JsonPropertyName("modeB64")]
    public string ModeB64 { get; set; } = "";

    [JsonPropertyName("savedAt")]
    public string SavedAt { get; set; } = "";

    public static TopologyBlob From(DisplayConfigPathInfo[] paths, DisplayConfigModeInfo[] modes, uint queryFlags = CcdConstants.QueryFlags)
    {
        return new TopologyBlob
        {
            Version = 1,
            QueryFlags = queryFlags,
            PathCount = paths.Length,
            ModeCount = modes.Length,
            PathB64 = Convert.ToBase64String(StructBytes(paths)),
            ModeB64 = Convert.ToBase64String(StructBytes(modes)),
            SavedAt = DateTime.UtcNow.ToString("o"),
        };
    }

    public (DisplayConfigPathInfo[] Paths, DisplayConfigModeInfo[] Modes) ToArrays()
    {
        var pathRaw = Convert.FromBase64String(PathB64);
        var modeRaw = Convert.FromBase64String(ModeB64);
        var expectedPath = Marshal.SizeOf<DisplayConfigPathInfo>() * PathCount;
        var expectedMode = Marshal.SizeOf<DisplayConfigModeInfo>() * ModeCount;
        if (pathRaw.Length != expectedPath || modeRaw.Length != expectedMode)
        {
            throw new InvalidOperationException(
                $"topology size mismatch: paths {pathRaw.Length}!={expectedPath}, modes {modeRaw.Length}!={expectedMode}");
        }

        return (FromBytes<DisplayConfigPathInfo>(pathRaw, PathCount), FromBytes<DisplayConfigModeInfo>(modeRaw, ModeCount));
    }

    public string ToJson() => JsonSerializer.Serialize(this, SessionJson.Options);

    public static TopologyBlob FromJson(string json) =>
        JsonSerializer.Deserialize<TopologyBlob>(json, SessionJson.Options)
        ?? throw new InvalidOperationException("invalid topology json");

    public static void Save(string path, DisplayConfigPathInfo[] paths, DisplayConfigModeInfo[] modes)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path)) ?? ".");
        File.WriteAllText(path, From(paths, modes).ToJson());
    }

    public static (DisplayConfigPathInfo[] Paths, DisplayConfigModeInfo[] Modes) Load(string path) =>
        FromJson(File.ReadAllText(path)).ToArrays();

    public static string Fingerprint(DisplayConfigPathInfo[] paths, DisplayConfigModeInfo[] modes)
    {
        var bytes = StructBytes(paths).Concat(StructBytes(modes)).ToArray();
        return Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();
    }

    public static byte[] StructBytes<T>(T[] items) where T : struct
    {
        var size = Marshal.SizeOf<T>();
        var dest = new byte[size * items.Length];
        var ptr = Marshal.AllocHGlobal(size);
        try
        {
            for (var i = 0; i < items.Length; i++)
            {
                Marshal.StructureToPtr(items[i], ptr, false);
                Marshal.Copy(ptr, dest, i * size, size);
                Marshal.DestroyStructure<T>(ptr);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(ptr);
        }

        return dest;
    }

    public static T[] FromBytes<T>(byte[] raw, int count) where T : struct
    {
        var size = Marshal.SizeOf<T>();
        var items = new T[count];
        var ptr = Marshal.AllocHGlobal(size);
        try
        {
            for (var i = 0; i < count; i++)
            {
                Marshal.Copy(raw, i * size, ptr, size);
                items[i] = Marshal.PtrToStructure<T>(ptr);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(ptr);
        }

        return items;
    }
}

internal static class SessionJson
{
    public static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = true,
        PropertyNameCaseInsensitive = true,
    };
}
