param(
    [switch]$Force,
    [string]$ManifestPath = (Join-Path $PSScriptRoot "payload.manifest.json"),
    [string]$PayloadRoot = (Join-Path $PSScriptRoot "payload")
)

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$ps = Join-Path $env:WINDIR "System32\WindowsPowerShell\v1.0\powershell.exe"
$validate = Join-Path $PSScriptRoot "ValidatePayload.ps1"

if (-not (Test-Path -LiteralPath $ManifestPath)) {
    throw "Missing $ManifestPath"
}

$manifest = Get-Content -LiteralPath $ManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json

$missing = @()
foreach ($name in $manifest.files.PSObject.Properties.Name) {
    $full = Join-Path $PayloadRoot ($name -replace "/", [IO.Path]::DirectorySeparatorChar)
    if (-not (Test-Path -LiteralPath $full)) {
        $missing += $name
    }
}

if (-not $Force -and $missing.Count -eq 0) {
    & $ps -NoProfile -ExecutionPolicy Bypass -File $validate -ManifestPath $ManifestPath -PayloadRoot $PayloadRoot
    if ($LASTEXITCODE -ne 0) {
        throw "Existing payload failed validation. Fix the files or re-run with -Force."
    }
    Write-Output "payload already valid; skip download (use -Force to refresh)."
    exit 0
}

if (-not $manifest.sources -or -not $manifest.sources.vddZip -or -not $manifest.sources.nefconZip) {
    throw "payload.manifest.json is missing sources.vddZip / sources.nefconZip"
}

$work = Join-Path $env:TEMP ("veil-payload-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $work | Out-Null

try {
    function Get-VeilZip {
        param([string]$Url, [string]$OutFile)
        Write-Output "downloading $Url"
        Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing -Headers @{
            "User-Agent" = "Veil-FetchPayload"
        }
        if (-not (Test-Path -LiteralPath $OutFile)) {
            throw "Download failed: $Url"
        }
    }

    function Find-VeilArchiveFile {
        param(
            [string]$Root,
            [string]$FileName,
            [string]$PreferPathSubstring
        )
        $hits = @(Get-ChildItem -LiteralPath $Root -Recurse -File | Where-Object { $_.Name -eq $FileName })
        if ($PreferPathSubstring) {
            $preferred = @($hits | Where-Object { $_.FullName.IndexOf($PreferPathSubstring, [StringComparison]::OrdinalIgnoreCase) -ge 0 })
            if ($preferred.Count -ge 1) {
                return $preferred[0]
            }
        }
        if ($hits.Count -eq 1) {
            return $hits[0]
        }
        if ($hits.Count -eq 0) {
            throw "Archive is missing $FileName"
        }
        throw "Archive has more than one $FileName"
    }

    $vddZip = Join-Path $work "vdd.zip"
    $nefZip = Join-Path $work "nefcon.zip"
    Get-VeilZip -Url ([string]$manifest.sources.vddZip) -OutFile $vddZip
    Get-VeilZip -Url ([string]$manifest.sources.nefconZip) -OutFile $nefZip

    $vddOut = Join-Path $work "vdd-extract"
    $nefOut = Join-Path $work "nefcon-extract"
    Expand-Archive -LiteralPath $vddZip -DestinationPath $vddOut -Force
    Expand-Archive -LiteralPath $nefZip -DestinationPath $nefOut -Force

    $staging = Join-Path $work "staging"
    $map = @(
        @{ Dest = "vdd\mttvdd.cat"; Root = $vddOut; Name = "mttvdd.cat"; Prefer = $null }
        @{ Dest = "vdd\MttVDD.dll"; Root = $vddOut; Name = "MttVDD.dll"; Prefer = $null }
        @{ Dest = "vdd\MttVDD.inf"; Root = $vddOut; Name = "MttVDD.inf"; Prefer = $null }
        @{ Dest = "nefcon\x64\nefconc.exe"; Root = $nefOut; Name = "nefconc.exe"; Prefer = "x64" }
    )
    foreach ($item in $map) {
        $src = Find-VeilArchiveFile -Root $item.Root -FileName $item.Name -PreferPathSubstring $item.Prefer
        $dest = Join-Path $staging $item.Dest
        $destDir = Split-Path -Parent $dest
        if (-not (Test-Path -LiteralPath $destDir)) {
            New-Item -ItemType Directory -Path $destDir | Out-Null
        }
        Copy-Item -LiteralPath $src.FullName -Destination $dest -Force
    }

    & $ps -NoProfile -ExecutionPolicy Bypass -File $validate -ManifestPath $ManifestPath -PayloadRoot $staging
    if ($LASTEXITCODE -ne 0) {
        throw "Downloaded payload failed validation; installer/payload was not updated."
    }

    foreach ($item in $map) {
        $src = Join-Path $staging $item.Dest
        $dest = Join-Path $PayloadRoot $item.Dest
        $destDir = Split-Path -Parent $dest
        if (-not (Test-Path -LiteralPath $destDir)) {
            New-Item -ItemType Directory -Path $destDir | Out-Null
        }
        Copy-Item -LiteralPath $src -Destination $dest -Force
    }

    Write-Output "payload fetched and validated."
}
finally {
    if (Test-Path -LiteralPath $work) {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }
}
