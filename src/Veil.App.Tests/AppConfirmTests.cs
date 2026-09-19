using System.IO;

namespace Veil.App.Tests;

public sealed class AppConfirmTests
{
    [Fact]
    public void AppAsksBeforeEnablingBundledVdd()
    {
        var source = File.ReadAllText(FindSource("src", "Veil.App", "App.xaml.cs"));
        Assert.Contains("confirmEnableVdd: ConfirmEnableVdd", source);
        Assert.Contains("Gate.EnableVddReason", source);
        Assert.Contains("MessageBoxButton.OKCancel", source);
        Assert.Contains("RecoveryCoordinator.DisableVddFailed", source);
        var coordinator = File.ReadAllText(FindSource("src", "Veil.App", "RecoveryCoordinator.cs"));
        Assert.Contains("RecoveryExitReason", coordinator);
        Assert.Contains("_isAlive", coordinator);
    }

    private static string FindSource(params string[] parts)
    {
        var dir = AppContext.BaseDirectory;
        for (var i = 0; i < 10; i++)
        {
            var candidate = Path.Combine(new[] { dir }.Concat(parts).ToArray());
            if (File.Exists(candidate))
            {
                return candidate;
            }

            dir = Path.GetFullPath(Path.Combine(dir, ".."));
        }

        throw new FileNotFoundException(string.Join('/', parts));
    }
}
