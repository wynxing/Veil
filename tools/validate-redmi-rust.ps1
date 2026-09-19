param(
    [int]$Seconds = 15,
    [string]$EvidenceRoot,
    [switch]$Release,
    [switch]$WaitHotkey
)

$ErrorActionPreference = "Stop"
$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repo

if (-not $EvidenceRoot) {
    $gitCommon = (git rev-parse --git-common-dir).Trim()
    if (-not [System.IO.Path]::IsPathRooted($gitCommon)) {
        $gitCommon = Join-Path $repo $gitCommon
    }
    $EvidenceRoot = Join-Path $gitCommon "veil-validation-20260919-rust-redmi"
}

$stamp = Get-Date -Format "HHmmss"
$run = Join-Path $EvidenceRoot ("short-" + $stamp)
New-Item -ItemType Directory -Force -Path (Join-Path $run "before") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $run "during") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $run "after") | Out-Null

function Invoke-Enumerate([string]$Dest) {
    python (Join-Path $repo "tools\display-probe\probe.py") enumerate |
        Set-Content -LiteralPath $Dest -Encoding utf8
}

function Write-Pnp([string]$Dest) {
    Get-PnpDevice -Class Display | ForEach-Object {
        $prob = (Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName DEVPKEY_Device_ProblemCode -ErrorAction SilentlyContinue).Data
        "{0} | {1} | problem={2} | {3}" -f $_.Status, $_.FriendlyName, $prob, $_.InstanceId
    } | Set-Content -LiteralPath $Dest -Encoding utf8
}

Write-Output ("evidence {0}" -f $run)
Write-Output ("UAC then keep-off about {0}s. Hotkey Ctrl+Alt+Shift+F10. Watch the lid." -f $Seconds)

Invoke-Enumerate (Join-Path $run "before\enumerate.txt")
Write-Pnp (Join-Path $run "before\pnp.txt")
& (Join-Path $repo "tools\display-probe\collect-evidence.ps1") -OutputDirectory (Join-Path $run "before")

$manifest = Join-Path $repo "src\Cargo.toml"
if ($Release) {
    cargo build --manifest-path $manifest --release
} else {
    cargo build --manifest-path $manifest
}
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$rel = Join-Path $repo "src\target\debug"
if ($Release) { $rel = Join-Path $repo "src\target\release" }
$stage = Join-Path $repo "installer\out\app"
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item (Join-Path $rel "veil_app.exe") (Join-Path $stage "Veil.App.exe") -Force
Copy-Item (Join-Path $rel "veil_recovery.exe") (Join-Path $stage "Veil.Recovery.exe") -Force
Copy-Item (Join-Path $rel "veil_driver_helper.exe") (Join-Path $stage "Veil.DriverHelper.exe") -Force
Copy-Item (Join-Path $env:ProgramFiles "Veil\payload.manifest.json") (Join-Path $stage "payload.manifest.json") -Force

$sessions = Join-Path $env:LOCALAPPDATA "Veil"
$known = @()
if (Test-Path $sessions) {
    $known = @(Get-ChildItem $sessions -Directory | ForEach-Object { $_.Name })
}

$flag = Join-Path $run "during\sampler.flag"
Set-Content -LiteralPath $flag -Value "1"
$during = Join-Path $run "during"
$sampler = Start-Job -ScriptBlock {
    param($Repo, $During, $Flag)
    $i = 0
    while (Test-Path -LiteralPath $Flag) {
        python (Join-Path $Repo "tools\display-probe\probe.py") enumerate |
            Set-Content -LiteralPath (Join-Path $During ("enumerate-{0}.txt" -f $i)) -Encoding utf8
        $i++
        Start-Sleep -Seconds 1
    }
} -ArgumentList $repo.Path, $during, $flag

$log = Join-Path $run "validate.log"
$app = Join-Path $stage "Veil.App.exe"
$validateArgs = @("--validate-keep-off-internal", "--seconds", "$Seconds", "--log", $log)
if ($WaitHotkey) { $validateArgs += "--wait-hotkey" }
$proc = Start-Process -FilePath $app -ArgumentList $validateArgs -WorkingDirectory $stage -Verb RunAs -PassThru
if (-not $proc) { throw "UAC denied or validate process missing" }

$deadline = (Get-Date).AddSeconds($Seconds + 25)
while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 400
    $proc.Refresh()
}

$newest = $null
if (Test-Path $sessions) {
    $newest = Get-ChildItem $sessions -Directory |
        Where-Object { $known -notcontains $_.Name -and $_.Name -like "session-*" } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
}

if ($newest -and -not (Test-Path (Join-Path $newest.FullName "result.json"))) {
    '{"at":0}' | Set-Content -LiteralPath (Join-Path $newest.FullName "release.json") -Encoding ascii
}

if (-not $proc.HasExited) {
    Wait-Process -Id $proc.Id -Timeout 30 -ErrorAction SilentlyContinue
}

Remove-Item -LiteralPath $flag -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1
Receive-Job $sampler | Out-Null
Stop-Job $sampler -ErrorAction SilentlyContinue
Remove-Job $sampler -Force -ErrorAction SilentlyContinue

Invoke-Enumerate (Join-Path $run "after\enumerate.txt")
Write-Pnp (Join-Path $run "after\pnp.txt")

if ($newest) {
    $sessDest = Join-Path $run $newest.Name
    Copy-Item $newest.FullName $sessDest -Recurse -Force
    Write-Output ("session {0}" -f $newest.Name)
}

$code = 1
if ($proc) { $code = $proc.ExitCode }
Write-Output ("exit {0}" -f $code)
Write-Output $run
