param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [datetime]$Since = (Get-Date).AddHours(-2)
)
$ErrorActionPreference = 'Stop'
$null = New-Item -ItemType Directory -Force -Path $OutputDirectory
$baseline = [ordered]@{
    CapturedAt = (Get-Date).ToUniversalTime().ToString('o')
    OS = Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber, OSArchitecture
    GPU = @(Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion, PNPDeviceID)
    Battery = @(Get-CimInstance -Namespace root/wmi -ClassName BatteryStatus | Select-Object PowerOnline, Charging, Discharging, RemainingCapacity)
    Devices = @(Get-PnpDevice -Class Display,Monitor | Select-Object Status, Class, FriendlyName, InstanceId)
    Drivers = @(Get-CimInstance Win32_PnPSignedDriver | Where-Object { $_.DeviceClass -in @('DISPLAY','MONITOR') } | Select-Object DeviceName, DeviceID, DriverVersion, InfName, IsSigned, Signer)
}
$baseline | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'baseline.json') -Encoding utf8
powercfg /a | Out-File -LiteralPath (Join-Path $OutputDirectory 'sleep-states.txt') -Encoding utf8
powercfg /q | Out-File -LiteralPath (Join-Path $OutputDirectory 'power-policy.txt') -Encoding utf8
$records = @()
$errorsFound = @()
foreach ($provider in @('Microsoft-Windows-Kernel-Power', 'Microsoft-Windows-Power-Troubleshooter')) {
    try {
        $records += @(Get-WinEvent -FilterHashtable @{LogName='System'; ProviderName=$provider; StartTime=$Since} -ErrorAction Stop | ForEach-Object {
            [ordered]@{TimeCreated=$_.TimeCreated.ToUniversalTime().ToString('o'); Id=$_.Id; Provider=$_.ProviderName; Message=$_.Message; Xml=$_.ToXml()}
        })
    } catch {
        $errorsFound += $_.Exception.Message
    }
}
@{Since=$Since.ToUniversalTime().ToString('o'); Records=$records; QueryNotes=$errorsFound} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'power-events.json') -Encoding utf8
