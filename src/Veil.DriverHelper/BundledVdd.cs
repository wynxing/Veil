using System.Runtime.InteropServices;
using System.Text;
using Veil.Engine.Native;

namespace Veil.DriverHelper;

internal static class BundledVdd
{
    public const string HardwareId = CcdConstants.BundledHardwareId;

    public static IReadOnlyList<string> FindInstanceIds()
    {
        var found = new List<string>();
        var guid = DisplayClassGuid;
        var set = Native.SetupDiGetClassDevs(ref guid, IntPtr.Zero, IntPtr.Zero, Native.DigcfAllClasses);
        if (set == Native.InvalidHandle)
        {
            return found;
        }

        try
        {
            var data = new Native.SpDevInfoData { CbSize = Marshal.SizeOf<Native.SpDevInfoData>() };
            for (uint i = 0; Native.SetupDiEnumDeviceInfo(set, i, ref data); i++)
            {
                var ids = GetHardwareIds(set, data);
                if (!ids.Any(id => string.Equals(id, HardwareId, StringComparison.OrdinalIgnoreCase)))
                {
                    continue;
                }

                var instance = GetInstanceId(set, data);
                if (!string.IsNullOrEmpty(instance))
                {
                    found.Add(instance);
                }
            }
        }
        finally
        {
            Native.SetupDiDestroyDeviceInfoList(set);
        }

        return found;
    }

    public static int EnableAll()
    {
        if (!PayloadPresent())
        {
            Console.Error.WriteLine("自带 VDD 文件缺失或哈希不符，拒绝启用。");
            return 2;
        }

        var ids = FindInstanceIds();
        if (ids.Count == 0)
        {
            Console.Error.WriteLine("未找到自带 Root\\MttVDD 设备。");
            return 2;
        }

        var rc = 0;
        foreach (var id in ids)
        {
            rc |= ChangeState(id, enable: true);
        }

        return rc;
    }

    public static int DisableAll()
    {
        var ids = FindInstanceIds();
        if (ids.Count == 0)
        {
            return 0;
        }

        var rc = 0;
        foreach (var id in ids)
        {
            rc |= ChangeState(id, enable: false);
        }

        return rc;
    }

    public static int ChangeState(string instanceId, bool enable)
    {
        var locate = Native.CM_Locate_DevNode(out var devInst, instanceId, 0);
        if (locate != 0)
        {
            Console.Error.WriteLine($"CM_Locate_DevNode {instanceId} -> {locate}");
            return 1;
        }

        var rc = enable ? Native.CM_Enable_DevNode(devInst, 0) : Native.CM_Disable_DevNode(devInst, 0);
        if (rc != 0)
        {
            Console.Error.WriteLine($"{(enable ? "enable" : "disable")} {instanceId} -> {rc}");
            return 1;
        }

        return 0;
    }

    public static bool PayloadPresent()
    {
        var program = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "Veil");
        var vdd = Path.Combine(program, "vdd");
        var dll = Path.Combine(vdd, "MttVDD.dll");
        var inf = Path.Combine(vdd, "MttVDD.inf");
        var cat = Path.Combine(vdd, "mttvdd.cat");
        if (!File.Exists(dll) || !File.Exists(inf) || !File.Exists(cat))
        {
            return false;
        }

        var manifest = Path.Combine(program, "payload.manifest.json");
        if (!File.Exists(manifest))
        {
            manifest = Path.Combine(AppContext.BaseDirectory, "payload.manifest.json");
        }

        if (!File.Exists(manifest))
        {
            return false;
        }

        using var doc = System.Text.Json.JsonDocument.Parse(File.ReadAllText(manifest));
        var files = doc.RootElement.GetProperty("files");
        var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["vdd/MttVDD.dll"] = dll,
            ["vdd/MttVDD.inf"] = inf,
            ["vdd/mttvdd.cat"] = cat,
        };
        foreach (var pair in map)
        {
            var expected = files.GetProperty(pair.Key).GetString();
            var actual = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(File.ReadAllBytes(pair.Value)));
            if (!string.Equals(expected, actual, StringComparison.OrdinalIgnoreCase))
            {
                return false;
            }
        }

        return true;
    }

    private static Guid DisplayClassGuid = new("4d36e968-e325-11ce-bfc1-08002be10318");

    private static string[] GetHardwareIds(IntPtr set, Native.SpDevInfoData data)
    {
        Native.SetupDiGetDeviceRegistryProperty(set, ref data, Native.SpdrpHardwareId, out _, IntPtr.Zero, 0, out var size);
        if (size == 0)
        {
            return [];
        }

        var buffer = Marshal.AllocHGlobal((int)size);
        try
        {
            if (!Native.SetupDiGetDeviceRegistryProperty(set, ref data, Native.SpdrpHardwareId, out _, buffer, size, out _))
            {
                return [];
            }

            return Marshal.PtrToStringUni(buffer)?.Split('\0', StringSplitOptions.RemoveEmptyEntries) ?? [];
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    private static string GetInstanceId(IntPtr set, Native.SpDevInfoData data)
    {
        var sb = new StringBuilder(1024);
        return Native.SetupDiGetDeviceInstanceId(set, ref data, sb, sb.Capacity, out _) ? sb.ToString() : "";
    }
}

internal static class Native
{
    public static readonly IntPtr InvalidHandle = new(-1);
    public const uint DigcfAllClasses = 0x00000004;
    public const uint SpdrpHardwareId = 0x00000001;

    [StructLayout(LayoutKind.Sequential)]
    public struct SpDevInfoData
    {
        public int CbSize;
        public Guid ClassGuid;
        public uint DevInst;
        public nint Reserved;
    }

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr SetupDiGetClassDevs(ref Guid classGuid, IntPtr enumerator, IntPtr hwndParent, uint flags);

    [DllImport("setupapi.dll", SetLastError = true)]
    public static extern bool SetupDiEnumDeviceInfo(IntPtr deviceInfoSet, uint memberIndex, ref SpDevInfoData deviceInfoData);

    [DllImport("setupapi.dll", SetLastError = true)]
    public static extern bool SetupDiDestroyDeviceInfoList(IntPtr deviceInfoSet);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool SetupDiGetDeviceRegistryProperty(
        IntPtr deviceInfoSet,
        ref SpDevInfoData deviceInfoData,
        uint property,
        out uint propertyRegDataType,
        IntPtr propertyBuffer,
        uint propertyBufferSize,
        out uint requiredSize);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool SetupDiGetDeviceInstanceId(
        IntPtr deviceInfoSet,
        ref SpDevInfoData deviceInfoData,
        StringBuilder deviceInstanceId,
        int deviceInstanceIdSize,
        out int requiredSize);

    [DllImport("cfgmgr32.dll", CharSet = CharSet.Unicode)]
    public static extern int CM_Locate_DevNode(out uint pdnDevInst, string pDeviceID, uint ulFlags);

    [DllImport("cfgmgr32.dll")]
    public static extern int CM_Enable_DevNode(uint dnDevInst, uint ulFlags);

    [DllImport("cfgmgr32.dll")]
    public static extern int CM_Disable_DevNode(uint dnDevInst, uint ulFlags);
}
