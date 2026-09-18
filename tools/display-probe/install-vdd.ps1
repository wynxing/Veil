#Requires -RunAsAdministrator
param([Parameter(Mandatory)][string]$EvidenceDirectory)

# One-machine validation helper. Pin the previously inspected upstream artifacts.
$ErrorActionPreference = 'Stop'
$veilBase = (Resolve-Path -LiteralPath $EvidenceDirectory).Path
$veilPackage = Join-Path $veilBase 'vdd\VirtualDisplayDriver'
$veilInstaller = Join-Path $veilBase 'nefcon\x64\nefconc.exe'
$veilDestination = 'C:\VirtualDisplayDriver'
$veilStatePath = Join-Path $veilBase 'vdd-install-state.json'
$veilState = [ordered]@{StartedAt=(Get-Date).ToUniversalTime().ToString('o'); Success=$false; CertificateAdded=$false; ConfigDirectoryCreated=$false}
try {
    if (Test-Path -LiteralPath $veilDestination) { throw 'Existing VDD directory; refusing to overwrite' }
    if (Test-Path -LiteralPath $veilStatePath) { throw 'Existing installation state; inspect before retrying' }
    $veilExisting = @(Get-PnpDevice -Class Display | Where-Object { $_.FriendlyName -match 'Virtual|MttVDD|IddSample' })
    if ($veilExisting.Count) { throw 'Existing virtual adapter; refusing to modify it' }
    $veilHashes = @{
        'mttvdd.cat'='08A0093FC9B2E32B287A6F8A77CA4DE0A31830D29FC33D2B13A918DC859468F6'
        'MttVDD.dll'='C9CA837F57A98FBD43BC416A7F535A95843626E7759EAF85CF0CD7CE334DBB05'
        'MttVDD.inf'='550D211FE481E74DFE3F9D724ED78BE48B3A9113405965D683D9373E8D672F5D'
    }
    foreach ($veilName in $veilHashes.Keys) {
        if ((Get-FileHash -LiteralPath (Join-Path $veilPackage $veilName) -Algorithm SHA256).Hash -ne $veilHashes[$veilName]) { throw "Hash mismatch: $veilName" }
    }
    if ((Get-FileHash -LiteralPath $veilInstaller -Algorithm SHA256).Hash -ne 'B65013F08BEF9D0DDCDEEF7501FC6BE346478B7B29B7730DE97C496408DDF9B4') { throw 'Installer hash mismatch' }
    foreach ($veilFile in @((Join-Path $veilPackage 'mttvdd.cat'), (Join-Path $veilPackage 'MttVDD.dll'), $veilInstaller)) {
        if ((Get-AuthenticodeSignature -FilePath $veilFile).Status -ne 'Valid') { throw "Invalid signature: $veilFile" }
    }
    $veilSigner = (Get-AuthenticodeSignature -FilePath (Join-Path $veilPackage 'mttvdd.cat')).SignerCertificate
    if ($veilSigner.Thumbprint -ne '3CF8CF26D8BA266C3A483AB7D26D4A818E317D76') { throw 'Unexpected publisher' }
    $veilState.CertificateThumbprint = $veilSigner.Thumbprint
    $veilState.DevicesBefore = @(Get-PnpDevice -Class Display,Monitor | Select-Object Status,Class,FriendlyName,InstanceId)
    $veilState | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $veilStatePath -Encoding UTF8
    $null = New-Item -ItemType Directory -Path $veilDestination
    $veilState.ConfigDirectoryCreated = $true
    foreach ($veilName in $veilHashes.Keys) { Copy-Item -LiteralPath (Join-Path $veilPackage $veilName) -Destination $veilDestination }
    @'
<?xml version="1.0" encoding="utf-8"?>
<vdd_settings>
  <monitors><count>1</count></monitors>
  <gpu><friendlyname>default</friendlyname></gpu>
  <global><g_refresh_rate>60</g_refresh_rate></global>
  <resolutions><resolution><width>1920</width><height>1200</height><refresh_rate>60</refresh_rate></resolution></resolutions>
  <options><CustomEdid>false</CustomEdid><PreventSpoof>false</PreventSpoof><EdidCeaOverride>false</EdidCeaOverride><HardwareCursor>true</HardwareCursor><SDR10bit>false</SDR10bit><HDRPlus>false</HDRPlus><logging>false</logging><debuglogging>false</debuglogging></options>
</vdd_settings>
'@ | Set-Content -LiteralPath (Join-Path $veilDestination 'vdd_settings.xml') -Encoding UTF8
    $veilCertPath = 'Cert:\LocalMachine\TrustedPublisher\' + $veilSigner.Thumbprint
    if (-not (Test-Path -LiteralPath $veilCertPath)) {
        $veilExport = Join-Path $veilBase 'vdd-publisher.cer'
        [IO.File]::WriteAllBytes($veilExport, $veilSigner.Export([Security.Cryptography.X509Certificates.X509ContentType]::Cert))
        $null = Import-Certificate -FilePath $veilExport -CertStoreLocation 'Cert:\LocalMachine\TrustedPublisher'
        $veilState.CertificateAdded = $true
    }
    $veilState | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $veilStatePath -Encoding UTF8
    & $veilInstaller install (Join-Path $veilDestination 'MttVDD.inf') 'Root\MttVDD' --no-duplicates 2>&1 | Out-File -LiteralPath (Join-Path $veilBase 'vdd-install.log') -Encoding UTF8
    $veilState.InstallerExitCode = $LASTEXITCODE
    if ($LASTEXITCODE -ne 0) { throw "Installer exit code $LASTEXITCODE; no automatic reboot or retry" }
    Start-Sleep -Seconds 3
    $veilState.DevicesAfter = @(Get-PnpDevice -Class Display,Monitor | Select-Object Status,Class,FriendlyName,InstanceId)
    $veilState.DriversAfter = @(Get-CimInstance Win32_PnPSignedDriver | Where-Object { $_.DeviceName -match 'Virtual Display|MttVDD' } | Select-Object DeviceName,DeviceID,InfName,DriverVersion,IsSigned,Signer)
    $veilState.Success = $true
} catch {
    $veilState.Error = $_.Exception.Message
} finally {
    $veilState.FinishedAt = (Get-Date).ToUniversalTime().ToString('o')
    # Retain the first installation's ownership record on a rejected rerun.
    if ((Test-Path -LiteralPath $veilStatePath) -and -not $veilState.Contains('DevicesBefore')) {
        $veilStatePath = Join-Path $veilBase ('vdd-install-rejected-' + [guid]::NewGuid().ToString('N') + '.json')
    }
    $veilState | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $veilStatePath -Encoding UTF8
}
if (-not $veilState.Success) { exit 1 }
