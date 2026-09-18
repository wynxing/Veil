namespace Veil.Engine;

public static class Roles
{
    private static readonly string[] VirtualNeedles =
    [
        @"root\display",
        "root#display",
        "iddcx",
        "virtual",
        "usbmmidd",
        "virtualdisplay",
        "indirect",
        "idd sample",
        "mtt",
    ];

    public static bool IsInternalTechnology(uint outputTechnology) =>
        outputTechnology is Native.CcdConstants.OutputTechnologyInternal
            or Native.CcdConstants.OutputTechnologyDisplayPortEmbedded
            or Native.CcdConstants.OutputTechnologyUdiEmbedded;

    public static bool LooksVirtual(string adapterPath, string monitorPath, string monitorName, string sourceName)
    {
        var blob = $"{adapterPath} {monitorPath} {monitorName} {sourceName}".ToLowerInvariant();
        return VirtualNeedles.Any(blob.Contains);
    }

    public static bool IsBundledVdd(string adapterPath, string monitorPath, string monitorName)
    {
        var adapter = adapterPath.ToLowerInvariant();
        var monitor = monitorPath.ToLowerInvariant();
        return adapter.Contains(@"root\mttvdd")
            || adapter.Contains("root#mttvdd")
            || adapter.Contains("mttvdd")
            || monitor.Contains("mtt1337")
            || monitor.Contains("mttvdd");
    }

    public static PathRole Classify(
        bool placeholder,
        bool internalTechnology,
        string adapterPath,
        string monitorPath,
        string monitorName,
        string sourceName)
    {
        if (placeholder)
        {
            return PathRole.Placeholder;
        }

        if (LooksVirtual(adapterPath, monitorPath, monitorName, sourceName))
        {
            return PathRole.Virtual;
        }

        if (internalTechnology)
        {
            return PathRole.Internal;
        }

        return PathRole.External;
    }
}
