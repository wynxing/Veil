$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $root) { $root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path }
if ($PSScriptRoot.EndsWith("installer")) {
    $root = Split-Path -Parent $PSScriptRoot
}

$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repo

$ps = Join-Path $env:WINDIR "System32\WindowsPowerShell\v1.0\powershell.exe"
& $ps -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "ValidatePayload.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$dotnet = Join-Path $env:ProgramFiles "dotnet\dotnet.exe"
if (-not (Test-Path $dotnet)) { throw "未找到 .NET SDK： $dotnet" }

$out = Join-Path $PSScriptRoot "out\app"
foreach ($proj in @(
        (Join-Path $repo "src\Veil.App\Veil.App.csproj"),
        (Join-Path $repo "src\Veil.Recovery\Veil.Recovery.csproj"),
        (Join-Path $repo "src\Veil.DriverHelper\Veil.DriverHelper.csproj")
    )) {
    & $dotnet publish $proj -c Release -p:Platform=x64 -o $out
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
Copy-Item (Join-Path $PSScriptRoot "payload.manifest.json") (Join-Path $out "payload.manifest.json") -Force

$wix = Join-Path $PSScriptRoot "Veil.Setup\Veil.Setup.wixproj"
& $dotnet build $wix -c Release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$bundle = Join-Path $PSScriptRoot "Veil.Setup\Veil.Bundle.wixproj"
if (Test-Path $bundle) {
    & $dotnet build $bundle -c Release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
