$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Get-VeilVersion.ps1')

function Assert-VeilCargoVersionMatches {
    param(
        [Parameter(Mandatory = $true)][string]$PropsPath,
        [Parameter(Mandatory = $true)][string]$CargoPath
    )

    $version = Get-VeilVersion -PropsPath $PropsPath
    if (-not (Test-Path -LiteralPath $CargoPath)) {
        throw "Cargo.toml not found: $CargoPath"
    }
    $text = Get-Content -LiteralPath $CargoPath -Raw -Encoding UTF8
    if ($text -notmatch '(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"') {
        throw 'Cargo.toml is missing workspace version.'
    }
    if ($Matches[1] -ne $version.Version) {
        throw ("Cargo.toml version {0} does not match version.props {1}." -f $Matches[1], $version.Version)
    }
    $cargoSuffix = $null
    if ($text -match '(?m)^suffix\s*=\s*"([^"]*)"') {
        $cargoSuffix = $Matches[1].Trim()
        if (-not $cargoSuffix) { $cargoSuffix = $null }
    }
    if ([string]$cargoSuffix -ne [string]$version.VersionSuffix) {
        throw ("Cargo.toml suffix '{0}' does not match version.props '{1}'." -f $cargoSuffix, $version.VersionSuffix)
    }
}

$repoRoot = Split-Path -Parent $PSScriptRoot
Assert-VeilCargoVersionMatches (Join-Path $repoRoot 'src\version.props') (Join-Path $repoRoot 'src\Cargo.toml')

$helper = Join-Path $PSScriptRoot 'Test-VeilPublishedRelease.ps1'
if (-not (Test-Path $helper)) { throw 'Missing release preflight: duplicate releases are not handled.' }
. $helper

# Only replace external GitHub/Git transport; execute the real release policy.
function gh {
    if (($args -join ' ') -ne 'api repos/{owner}/{repo}/releases --paginate --slurp') { throw "Unexpected gh call: $args" }
    $global:LASTEXITCODE = $script:apiExit
    $script:response
}
function git {
    if (($args -join ' ') -ne 'ls-remote --exit-code origin refs/tags/v1.2.3-preview.1 refs/tags/v1.2.3-preview.1^{}') { throw "Unexpected git call: $args" }
    $global:LASTEXITCODE = $script:gitExit
    $script:refs
}
function Assert-Throws($Action, $Pattern) {
    try { & $Action } catch {
        if ($_.Exception.Message -notmatch $Pattern) { throw }
        return
    }
    throw "Expected failure matching $Pattern"
}
function Check { Test-VeilPublishedRelease -Tag 'v1.2.3-preview.1' -SetupFileName 'VeilSetup-1.2.3-preview.1-x64.exe' -Head 'abc123' }
$script:apiExit = 0
$script:gitExit = 0
$script:refs = "abc123`trefs/tags/v1.2.3-preview.1"
$script:response = '[[]]'
if (Check) { throw 'Absent release must proceed to build.' }
$script:response = '[[{"tag_name":"v1.2.3-preview.1","draft":false,"assets":[{"name":"VeilSetup-1.2.3-preview.1-x64.exe","state":"uploaded","size":42},{"name":"SHA256SUMS.txt","state":"uploaded","size":101}]}]]'
$complete = $script:response
if (-not (Check)) { throw 'Complete release must be skipped.' }
$script:response = '[[], ' + $complete.Substring(1)
if (-not (Check)) { throw 'Existing release on a later page must be skipped.' }
$script:response = $complete
$script:refs = @("tagobject`trefs/tags/v1.2.3-preview.1", "abc123`trefs/tags/v1.2.3-preview.1^{}")
if (-not (Check)) { throw 'Annotated tag must resolve to its commit.' }
$script:refs = "different`trefs/tags/v1.2.3-preview.1"
Assert-Throws { Check } 'points to'
$script:refs = "abc123`trefs/tags/v1.2.3-preview.1"
$script:gitExit = 2
Assert-Throws { Check } 'remote tag'
$script:gitExit = 0
$script:response = $complete.Replace('"draft":false', '"draft":true')
Assert-Throws { Check } 'draft'
$script:response = $complete.Replace('SHA256SUMS.txt', 'wrong.txt')
Assert-Throws { Check } 'incomplete'
$script:response = $complete.Replace('"size":42', '"size":0')
Assert-Throws { Check } 'incomplete'
$script:response = $complete.Replace('"state":"uploaded"', '"state":"starter"')
Assert-Throws { Check } 'incomplete'
$script:response = '[[]]'
$script:apiExit = 1
Assert-Throws { Check } 'query'
$script:apiExit = 0
$script:response = 'not json'
Assert-Throws { Check } 'JSON|json|Unexpected|Invalid'

$mismatch = Join-Path ([System.IO.Path]::GetTempPath()) ("veil-version-" + [guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory -Path $mismatch | Out-Null
try {
    [System.IO.File]::WriteAllText((Join-Path $mismatch 'version.props'), "<Project><PropertyGroup><Version>9.9.9</Version><VersionSuffix>preview.1</VersionSuffix></PropertyGroup></Project>")
    [System.IO.File]::WriteAllText((Join-Path $mismatch 'Cargo.toml'), "version = `"0.1.0`"`r`nsuffix = `"preview.1`"`r`n")
    Assert-Throws { Assert-VeilCargoVersionMatches (Join-Path $mismatch 'version.props') (Join-Path $mismatch 'Cargo.toml') } 'does not match'
} finally {
    Remove-Item -LiteralPath $mismatch -Recurse -Force
}

Write-Output 'Version sources match.'
Write-Output 'Release preflight: 12 cases passed.'
