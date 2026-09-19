using System.Text.Json;
using System.Text.Json.Serialization;

namespace Veil.Engine.Session;

public sealed class SessionEvent
{
    [JsonPropertyName("ts")]
    public string Ts { get; set; } = "";

    [JsonPropertyName("type")]
    public string Type { get; set; } = "";

    [JsonPropertyName("detail")]
    public string? Detail { get; set; }

    [JsonPropertyName("reason")]
    public string? Reason { get; set; }

    [JsonPropertyName("reapply")]
    public bool? Reapply { get; set; }

    [JsonPropertyName("applyRc")]
    public int? ApplyRc { get; set; }
}

public static class SessionLog
{
    private static readonly JsonSerializerOptions LineOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = false,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    public static void Append(
        string directory,
        string type,
        string? detail = null,
        string? reason = null,
        bool? reapply = null,
        int? applyRc = null)
    {
        var ev = new SessionEvent
        {
            Ts = DateTimeOffset.UtcNow.ToString("o"),
            Type = type,
            Detail = detail,
            Reason = reason,
            Reapply = reapply,
            ApplyRc = applyRc,
        };
        try
        {
            Directory.CreateDirectory(directory);
            File.AppendAllText(
                SessionPaths.Events(directory),
                JsonSerializer.Serialize(ev, LineOptions) + Environment.NewLine);
        }
        catch (IOException)
        {
        }
    }
}
