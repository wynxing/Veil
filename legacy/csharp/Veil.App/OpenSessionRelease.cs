using System.IO;
using Veil.Engine.Session;

namespace Veil.App;

public static class OpenSessionRelease
{
    public static void RequestAll()
    {
        var root = SessionPaths.Root;
        if (!Directory.Exists(root))
        {
            return;
        }

        foreach (var dir in Directory.EnumerateDirectories(root, "session-*"))
        {
            if (File.Exists(SessionPaths.Result(dir)))
            {
                continue;
            }

            JsonUtil.WriteAtomic(SessionPaths.Release(dir), new ReleaseFile { At = DateTimeOffset.UtcNow.ToUnixTimeSeconds() });
        }
    }
}
