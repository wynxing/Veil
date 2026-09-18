param(
    [string]$ManifestPath = (Join-Path $PSScriptRoot "payload.manifest.json"),
    [string]$PayloadRoot = (Join-Path $PSScriptRoot "payload")
)

$ErrorActionPreference = "Stop"
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
    $actual = (Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash
    $expected = [string]$manifest.files.$name
    if ($actual -ne $expected) {
        throw "Hash mismatch: $name actual $actual expected $expected"
    }
}

if ($missing.Count) {
    throw ("Installer payload missing: {0}. Place verified files in installer/payload/. See installer/payload/README.md" -f ($missing -join ", "))
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
