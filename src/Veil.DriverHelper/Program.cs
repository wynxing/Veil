using System.Diagnostics;
using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;
using System.Text.Json;
using Veil.Engine.Native;

namespace Veil.DriverHelper;

internal static class Program
{
    private static int Main(string[] args)
    {
        var verb = args.Length > 0 ? args[0] : "status";
        try
        {
            return verb switch
            {
                "status" => Status(),
                "enable" => BundledVdd.EnableAll(),
                "disable" => BundledVdd.DisableAll(),
                "install-driver" => InstallDriver(),
                "uninstall-driver" => UninstallDriver(),
                _ => Fail($"unknown verb {verb}"),
            };
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine(ex.Message);
            return 1;
        }
    }

    private static int Status()
    {
        var ids = BundledVdd.FindInstanceIds();
        Console.WriteLine(JsonSerializer.Serialize(new
        {
            hardwareId = BundledVdd.HardwareId,
            instances = ids,
            installed = ids.Count > 0,
        }));
        return 0;
    }

    private static int InstallDriver()
    {
        var payload = ResolvePayload();
        ValidatePayload(payload);
        WriteSettings(payload.VddDir);
        var inf = Path.Combine(payload.VddDir, "MttVDD.inf");
        var rc = Run(payload.Nefcon, $"install \"{inf}\" {CcdConstants.BundledHardwareId} --no-duplicates");
        if (rc != 0)
        {
            return rc;
        }

        Thread.Sleep(2000);
        return BundledVdd.DisableAll();
    }

    private static int UninstallDriver()
    {
        _ = BundledVdd.DisableAll();
        var payload = ResolvePayload();
        var inf = Path.Combine(payload.VddDir, "MttVDD.inf");
        if (File.Exists(payload.Nefcon) && File.Exists(inf))
        {
            return Run(payload.Nefcon, $"remove {CcdConstants.BundledHardwareId} --force");
        }

        return 0;
    }

    private static (string VddDir, string Nefcon) ResolvePayload()
    {
        var roots = new[]
        {
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "Veil"),
            Path.GetFullPath(Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "installer")),
            AppContext.BaseDirectory,
        };
        foreach (var root in roots)
        {
            var vdd = Path.Combine(root, "vdd");
            var nefcon = Path.Combine(root, "nefcon", "x64", "nefconc.exe");
            if (Directory.Exists(vdd) && File.Exists(nefcon))
            {
                return (vdd, nefcon);
            }
        }

        var program = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "Veil");
        return (Path.Combine(program, "vdd"), Path.Combine(program, "nefcon", "x64", "nefconc.exe"));
    }

    private static void ValidatePayload((string VddDir, string Nefcon) payload)
    {
        var manifestPath = FindManifest();
        if (manifestPath is null)
        {
            throw new InvalidOperationException("缺少 payload.manifest.json，拒绝安装驱动。");
        }

        using var doc = JsonDocument.Parse(File.ReadAllText(manifestPath));
        var thumb = doc.RootElement.GetProperty("publisherThumbprint").GetString();
        if (string.IsNullOrWhiteSpace(thumb))
        {
            throw new InvalidOperationException("manifest 缺少 publisherThumbprint");
        }
        var files = doc.RootElement.GetProperty("files");
        var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["vdd/mttvdd.cat"] = Path.Combine(payload.VddDir, "mttvdd.cat"),
            ["vdd/MttVDD.dll"] = Path.Combine(payload.VddDir, "MttVDD.dll"),
            ["vdd/MttVDD.inf"] = Path.Combine(payload.VddDir, "MttVDD.inf"),
            ["nefcon/x64/nefconc.exe"] = payload.Nefcon,
        };
        foreach (var pair in map)
        {
            if (!File.Exists(pair.Value))
            {
                throw new InvalidOperationException("缺少 " + pair.Key);
            }

            var expected = files.GetProperty(pair.Key).GetString();
            var actual = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(pair.Value)));
            if (!string.Equals(expected, actual, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException("哈希不符：" + pair.Key);
            }
        }

        foreach (var signed in new[] { Path.Combine(payload.VddDir, "mttvdd.cat"), Path.Combine(payload.VddDir, "MttVDD.dll"), payload.Nefcon })
        {
            try
            {
                using var cert = X509Certificate.CreateFromSignedFile(signed);
                if (string.IsNullOrEmpty(cert.Subject))
                {
                    throw new InvalidOperationException("无法读取签名：" + signed);
                }
            }
            catch (CryptographicException)
            {
                Console.Error.WriteLine("警告：运行时未能解析 Authenticode（" + signed + "）。哈希已核对；安装器脚本仍会核验签名。");
            }
        }
    }

    private static string? FindManifest()
    {
        var candidates = new[]
        {
            Path.Combine(AppContext.BaseDirectory, "payload.manifest.json"),
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "Veil", "payload.manifest.json"),
            Path.GetFullPath(Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "installer", "payload.manifest.json")),
        };
        return candidates.FirstOrDefault(File.Exists);
    }

    private static void WriteSettings(string vddDir)
    {
        Directory.CreateDirectory(vddDir);
        var xml = """
            <?xml version="1.0" encoding="utf-8"?>
            <vdd_settings>
              <monitors><count>1</count></monitors>
              <gpu><friendlyname>default</friendlyname></gpu>
              <global><g_refresh_rate>60</g_refresh_rate></global>
              <resolutions><resolution><width>1920</width><height>1200</height><refresh_rate>60</refresh_rate></resolution></resolutions>
              <options><CustomEdid>false</CustomEdid><PreventSpoof>false</PreventSpoof><EdidCeaOverride>false</EdidCeaOverride><HardwareCursor>true</HardwareCursor><SDR10bit>false</SDR10bit><HDRPlus>false</HDRPlus><logging>false</logging><debuglogging>false</debuglogging></options>
            </vdd_settings>
            """;
        File.WriteAllText(Path.Combine(vddDir, "vdd_settings.xml"), xml);
    }

    private static int Run(string file, string args)
    {
        using var proc = Process.Start(new ProcessStartInfo(file, args)
        {
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        });
        if (proc is null)
        {
            return 1;
        }

        proc.WaitForExit();
        Console.Write(proc.StandardOutput.ReadToEnd());
        Console.Error.Write(proc.StandardError.ReadToEnd());
        return proc.ExitCode;
    }

    private static int Fail(string message)
    {
        Console.Error.WriteLine(message);
        return 2;
    }
}
