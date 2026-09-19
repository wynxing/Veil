using Veil.Engine;

namespace Veil.Engine.Tests;

public sealed class BundledVddSettingsTests
{
    [Fact]
    public void DriverReadsHardcodedLabDirectory()
    {
        Assert.Equal(@"C:\VirtualDisplayDriver", BundledVddSettings.DriverReadsDirectory);
        Assert.Equal("vdd_settings.xml", BundledVddSettings.FileName);
        Assert.Contains("<count>1</count>", BundledVddSettings.Xml);
        Assert.Contains("<width>1920</width>", BundledVddSettings.Xml);
        Assert.Contains("<height>1200</height>", BundledVddSettings.Xml);
    }

    [Fact]
    public void WriteXmlCopiesIntoInstallAndDriverDirectories()
    {
        var root = Path.Combine(Path.GetTempPath(), "veil-vdd-settings-" + Guid.NewGuid().ToString("N"));
        var install = Path.Combine(root, "program-vdd");
        var driver = Path.Combine(root, "driver-read");
        try
        {
            BundledVddSettings.WriteXml(install, driver);
            var a = Path.Combine(install, BundledVddSettings.FileName);
            var b = Path.Combine(driver, BundledVddSettings.FileName);
            Assert.True(File.Exists(a));
            Assert.True(File.Exists(b));
            Assert.Equal(BundledVddSettings.Xml.Replace("\r\n", "\n"), File.ReadAllText(a).Replace("\r\n", "\n"));
            Assert.Equal(File.ReadAllText(a), File.ReadAllText(b));
            Assert.True(BundledVddSettings.TryRemoveOwnedFile(install));
            Assert.False(File.Exists(a));
            File.WriteAllText(b, "<vdd_settings>foreign</vdd_settings>");
            Assert.False(BundledVddSettings.TryRemoveOwnedFile(driver));
            Assert.True(File.Exists(b));
            Assert.Throws<InvalidOperationException>(() => BundledVddSettings.WriteXml(driver));
            var extra = Path.Combine(root, "rollback");
            var created = BundledVddSettings.WriteXml(extra);
            Assert.Single(created);
            BundledVddSettings.RollbackCreated(created);
            Assert.False(File.Exists(Path.Combine(extra, BundledVddSettings.FileName)));
        }
        finally
        {
            try
            {
                Directory.Delete(root, true);
            }
            catch (IOException)
            {
            }
        }
    }
}
