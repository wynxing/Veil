namespace Veil.Engine;

/// <summary>
/// MTT 25.7.23 的 <c>MttVDD.dll</c> 把配置目录写死为 <c>C:\VirtualDisplayDriver</c>（UTF-16 字符串核验）。
/// 安装器仍把 INF/DLL 放到 <c>%ProgramFiles%\Veil\vdd</c>；设置文件必须同时写到驱动读取的目录。
/// </summary>
public static class BundledVddSettings
{
    public const string DriverReadsDirectory = @"C:\VirtualDisplayDriver";
    public const string FileName = "vdd_settings.xml";

    public const string Xml = """
        <?xml version="1.0" encoding="utf-8"?>
        <vdd_settings>
          <monitors><count>1</count></monitors>
          <gpu><friendlyname>default</friendlyname></gpu>
          <global><g_refresh_rate>60</g_refresh_rate></global>
          <resolutions><resolution><width>1920</width><height>1200</height><refresh_rate>60</refresh_rate></resolution></resolutions>
          <options><CustomEdid>false</CustomEdid><PreventSpoof>false</PreventSpoof><EdidCeaOverride>false</EdidCeaOverride><HardwareCursor>true</HardwareCursor><SDR10bit>false</SDR10bit><HDRPlus>false</HDRPlus><logging>false</logging><debuglogging>false</debuglogging></options>
        </vdd_settings>
        """;

    public static IReadOnlyList<string> WriteXml(params string[] directories)
    {
        var created = new List<string>();
        foreach (var dir in directories.Where(d => !string.IsNullOrWhiteSpace(d)).Distinct(StringComparer.OrdinalIgnoreCase))
        {
            Directory.CreateDirectory(dir);
            var path = Path.Combine(dir, FileName);
            if (File.Exists(path))
            {
                if (!OwnsFile(path))
                {
                    throw new InvalidOperationException("已有他人的 " + FileName + "：" + path);
                }

                continue;
            }

            File.WriteAllText(path, Xml);
            created.Add(path);
        }

        return created;
    }

    public static bool OwnsFile(string path)
    {
        try
        {
            if (!File.Exists(path))
            {
                return false;
            }

            return Normalize(File.ReadAllText(path)) == Normalize(Xml);
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
    }

    /// <summary>
    /// 只删除内容与本产品写入的 XML 一致的设置文件，不删目录、不碰他人配置。
    /// </summary>
    public static bool TryRemoveOwnedFile(string directory)
    {
        if (string.IsNullOrWhiteSpace(directory))
        {
            return false;
        }

        var path = Path.Combine(directory, FileName);
        try
        {
            if (!OwnsFile(path))
            {
                return false;
            }

            File.Delete(path);
            return true;
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
    }

    public static void RollbackCreated(IEnumerable<string> paths)
    {
        foreach (var path in paths)
        {
            if (string.IsNullOrWhiteSpace(path))
            {
                continue;
            }

            var dir = Path.GetDirectoryName(path);
            if (!string.IsNullOrEmpty(dir))
            {
                TryRemoveOwnedFile(dir);
            }
        }
    }

    private static string Normalize(string text) => text.Replace("\r\n", "\n");
}
