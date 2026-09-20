param(
    [string]$ManifestPath = (Join-Path $PSScriptRoot "payload.manifest.json"),
    [string]$PayloadRoot = (Join-Path $PSScriptRoot "payload")
)

$ErrorActionPreference = "Stop"

# Resolve the module from this PowerShell runtime, not an inherited PS7/PS5 module path.
if (-not (Get-Module -Name Microsoft.PowerShell.Security)) {
    Import-Module (Join-Path $PSHOME "Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1") -ErrorAction Stop
}

function Get-VeilFileSha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        return ([BitConverter]::ToString($hasher.ComputeHash($stream)) -replace "-", "")
    }
    finally {
        $stream.Dispose()
        $hasher.Dispose()
    }
}

if (-not (Test-Path -LiteralPath $ManifestPath)) {
    throw "Missing $ManifestPath"
}

$manifest = Get-Content -LiteralPath $ManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
$missing = @()
foreach ($name in $manifest.files.PSObject.Properties.Name) {
    $full = Join-Path $PayloadRoot ($name -replace "/", [IO.Path]::DirectorySeparatorChar)
    if (-not (Test-Path -LiteralPath $full)) {
        $missing += $name
        continue
    }
    $actual = Get-VeilFileSha256 -Path $full
    $expected = [string]$manifest.files.$name
    if ($actual -ne $expected) {
        throw "Hash mismatch: $name actual $actual expected $expected"
    }
}

if ($missing.Count) {
    throw ("Installer payload missing: {0}. Run installer/FetchPayload.ps1 or place verified files in installer/payload/. See installer/payload/README.md" -f ($missing -join ", "))
}

foreach ($signedName in @("vdd/mttvdd.cat", "vdd/MttVDD.dll", "nefcon/x64/nefconc.exe")) {
    $full = Join-Path $PayloadRoot ($signedName -replace "/", [IO.Path]::DirectorySeparatorChar)
    $sig = Get-AuthenticodeSignature -FilePath $full
    if ($sig.Status -ne "Valid") {
        throw "Invalid signature: $signedName ($($sig.Status))"
    }
}

$cat = Join-Path $PayloadRoot "vdd\mttvdd.cat"
$thumb = (Get-AuthenticodeSignature -FilePath $cat).SignerCertificate.Thumbprint
if ($thumb -ne $manifest.publisherThumbprint) {
    throw "Publisher thumbprint mismatch: $thumb"
}

Write-Output "payload hash and signature checks passed."
