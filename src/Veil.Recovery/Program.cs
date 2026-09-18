using Veil.Engine.Native;
using Veil.Engine.Runtime;

namespace Veil.Recovery;

internal static class Program
{
    [STAThread]
    private static int Main(string[] args)
    {
        string? directory = null;
        var parentPid = 0;
        for (var i = 0; i < args.Length; i++)
        {
            if (args[i] == "--directory" && i + 1 < args.Length)
            {
                directory = args[++i];
            }
            else if (args[i] == "--parent-pid" && i + 1 < args.Length)
            {
                _ = int.TryParse(args[++i], out parentPid);
            }
        }

        if (string.IsNullOrWhiteSpace(directory))
        {
            return 2;
        }

        Directory.CreateDirectory(directory);
        var session = new RecoverySession(new RecoveryOptions
        {
            Directory = directory,
            ParentPid = parentPid,
            Ccd = new Win32CcdApi(),
            Hotkey = new Win32Hotkey(),
        });
        session.RunUntilExit();
        return session.Result.Ok ? 0 : 1;
    }
}
