$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "Get-VeilVersion.ps1")
$version = Get-VeilVersion
$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repo

$ps = Join-Path $env:WINDIR "System32\WindowsPowerShell\v1.0\powershell.exe"
$validate = Join-Path $PSScriptRoot "ValidatePayload.ps1"
$fetch = Join-Path $PSScriptRoot "FetchPayload.ps1"
$payloadRoot = Join-Path $PSScriptRoot "payload"

$needFetch = $false
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot "payload.manifest.json") -Raw -Encoding UTF8 | ConvertFrom-Json
foreach ($name in $manifest.files.PSObject.Properties.Name) {
    $full = Join-Path $payloadRoot ($name -replace "/", [IO.Path]::DirectorySeparatorChar)
    if (-not (Test-Path -LiteralPath $full)) {
        $needFetch = $true
        break
    }
}
if ($needFetch) {
    Write-Output "payload missing; running FetchPayload.ps1"
    & $ps -NoProfile -ExecutionPolicy Bypass -File $fetch
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

& $ps -NoProfile -ExecutionPolicy Bypass -File $validate
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) { throw "cargo not found. Install Rust MSVC toolchain." }

$outRoot = Join-Path $PSScriptRoot "out"
$out = Join-Path $outRoot "app"
if (Test-Path -LiteralPath $outRoot) {
    Remove-Item -LiteralPath $outRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $out | Out-Null

$manifest = Join-Path $repo "src\Cargo.toml"
& cargo build --manifest-path $manifest --release --target x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$releaseDir = Join-Path $repo "src\target\x86_64-pc-windows-msvc\release"
$copies = @(
    @{ Src = "veil_app.exe"; Dest = "Veil.App.exe" },
    @{ Src = "veil_recovery.exe"; Dest = "Veil.Recovery.exe" },
    @{ Src = "veil_driver_helper.exe"; Dest = "Veil.DriverHelper.exe" }
)
foreach ($item in $copies) {
    $from = Join-Path $releaseDir $item.Src
    if (-not (Test-Path -LiteralPath $from)) {
        $from = Join-Path (Join-Path $repo "src\target\release") $item.Src
    }
    if (-not (Test-Path -LiteralPath $from)) {
        throw "Rust release is missing $($item.Src)"
    }
    Copy-Item -LiteralPath $from -Destination (Join-Path $out $item.Dest) -Force
}
Copy-Item (Join-Path $PSScriptRoot "payload.manifest.json") (Join-Path $out "payload.manifest.json") -Force

$dotnet = Join-Path $env:ProgramFiles "dotnet\dotnet.exe"
if (-not (Test-Path $dotnet)) { throw "dotnet SDK not found (needed to compile WiX): $dotnet" }

$appHost = Join-Path $out "Veil.App.exe"
$recovery = Join-Path $out "Veil.Recovery.exe"
$helper = Join-Path $out "Veil.DriverHelper.exe"
foreach ($exe in @($appHost, $recovery, $helper)) {
    if (-not (Test-Path -LiteralPath $exe)) {
        throw "Self-contained publish is missing $exe"
    }
}

$wix = Join-Path $PSScriptRoot "Veil.Setup\Veil.Setup.wixproj"
& $dotnet build $wix -c Release -p:ProductVersion=$($version.Version)
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$bundle = Join-Path $PSScriptRoot "Veil.Bundle\Veil.Bundle.wixproj"
if (Test-Path $bundle) {
    & $dotnet restore $bundle
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & $dotnet build $bundle -c Release -p:ProductVersion=$($version.Version)
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$built = Join-Path $PSScriptRoot "Veil.Bundle\bin\Release\VeilSetup.exe"
if (-not (Test-Path -LiteralPath $built)) {
    throw "Missing bundle output: $built"
}

$dist = Join-Path $PSScriptRoot "dist"
if (-not (Test-Path -LiteralPath $dist)) {
    New-Item -ItemType Directory -Path $dist | Out-Null
}
$destExe = Join-Path $dist $version.SetupFileName
Copy-Item -LiteralPath $built -Destination $destExe -Force

$sumPath = Join-Path $dist "SHA256SUMS.txt"
$hasher = [System.Security.Cryptography.SHA256]::Create()
$stream = [System.IO.File]::OpenRead($destExe)
try {
    $hash = ([BitConverter]::ToString($hasher.ComputeHash($stream)) -replace "-", "")
}
finally {
    $stream.Dispose()
    $hasher.Dispose()
}
Set-Content -LiteralPath $sumPath -Encoding ASCII -Value ("{0}  {1}" -f $hash, $version.SetupFileName)

Write-Output ("packed {0} ({1})" -f $destExe, $version.Tag)
Write-Output ("sha256 {0}" -f $hash)
