namespace Veil.App.Tests;

public sealed class MainWindowAutomationIdsTests
{
    [Fact]
    public void RestoreButtonsHaveStableAutomationIds()
    {
        var xaml = System.IO.File.ReadAllText(MainWindowPath());
        Assert.Contains("AutomationProperties.AutomationId=\"VeilPanel\"", xaml);
        Assert.Contains("AutomationProperties.AutomationId=\"RestoreAllButton\"", xaml);
        Assert.Contains("AutomationProperties.AutomationId=\"{Binding KeepOffAutomationId}\"", xaml);
        Assert.Contains("AutomationProperties.AutomationId=\"{Binding RestoreAutomationId}\"", xaml);
    }

    private static string MainWindowPath()
    {
        var dir = AppContext.BaseDirectory;
        for (var i = 0; i < 10; i++)
        {
            var candidates = new[]
            {
                System.IO.Path.Combine(dir, "Veil.App", "MainWindow.xaml"),
                System.IO.Path.Combine(dir, "src", "Veil.App", "MainWindow.xaml"),
            };
            foreach (var candidate in candidates)
            {
                if (System.IO.File.Exists(candidate))
                {
                    return candidate;
                }
            }

            dir = System.IO.Path.GetFullPath(System.IO.Path.Combine(dir, ".."));
        }

        throw new System.IO.FileNotFoundException("MainWindow.xaml");
    }
}
