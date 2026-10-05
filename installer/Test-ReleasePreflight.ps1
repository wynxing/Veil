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
$script:testTag = 'v1.2.3-preview.1'
$script:testSetupName = 'VeilSetup-1.2.3-preview.1-x64.exe'
function gh {
    if ($global:VeilTestPublishing) {
        $global:LASTEXITCODE = 0
        '[[]]'
        return
    }
    if (($args -join ' ') -ne 'api repos/{owner}/{repo}/releases --paginate --slurp') { throw "Unexpected gh call: $args" }
    $global:LASTEXITCODE = $script:apiExit
    $script:response
}
function git {
    if ($global:VeilTestPublishing) {
        $global:LASTEXITCODE = 0
        switch ($args -join ' ') {
            'rev-parse HEAD' { 'abc123'; return }
            "ls-remote --exit-code origin refs/tags/$global:VeilTestPublishTag refs/tags/$global:VeilTestPublishTag^{}" { "abc123`trefs/tags/$global:VeilTestPublishTag"; return }
            "tag -l $global:VeilTestPublishTag" { $global:VeilTestPublishTag; return }
            "rev-parse $global:VeilTestPublishTag^{}" { 'abc123'; return }
            default { throw "Unexpected publish git call: $args" }
        }
    }
    if (($args -join ' ') -ne "ls-remote --exit-code origin refs/tags/$script:testTag refs/tags/$script:testTag^{}") { throw "Unexpected git call: $args" }
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
function Check { Test-VeilPublishedRelease -Tag $script:testTag -SetupFileName $script:testSetupName -Head 'abc123' }
$script:apiExit = 0
$script:gitExit = 0
$script:refs = "abc123`trefs/tags/v1.2.3-preview.1"
$script:response = '[[]]'
if (Check) { throw 'Absent release must proceed to build.' }
$script:refs = "different`trefs/tags/v1.2.3-preview.1"
Assert-Throws { Check } 'points to'
$script:gitExit = 2
$script:refs = @()
if (Check) { throw 'Absent release and absent tag must proceed to build.' }
$script:gitExit = 1
Assert-Throws { Check } 'remote tag'
$script:gitExit = 0
$script:refs = "abc123`trefs/tags/v1.2.3-preview.1"
$script:response = '[[{"tag_name":"v1.2.3-preview.1","draft":false,"prerelease":true,"assets":[{"name":"VeilSetup-1.2.3-preview.1-x64.exe","state":"uploaded","size":42},{"name":"SHA256SUMS.txt","state":"uploaded","size":101}]}]]'
$complete = $script:response
if (-not (Check)) { throw 'Complete release must be skipped.' }
$script:response = '[[], ' + $complete.Substring(1)
if (-not (Check)) { throw 'Existing release on a later page must be skipped.' }
$script:response = $complete.Replace('"prerelease":true', '"prerelease":false')
Assert-Throws { Check } 'channel|prerelease'
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

$script:testTag = 'v1.2.3'
$script:testSetupName = 'VeilSetup-1.2.3-x64.exe'
$script:refs = "abc123`trefs/tags/v1.2.3"
$script:response = $complete.Replace('1.2.3-preview.1', '1.2.3').Replace('"prerelease":true', '"prerelease":false')
if (-not (Check)) { throw 'Complete stable release must be skipped.' }
$script:response = $script:response.Replace('"prerelease":false', '"prerelease":true')
Assert-Throws { Check } 'channel|prerelease'

$mismatch = Join-Path ([System.IO.Path]::GetTempPath()) ("veil-version-" + [guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory -Path $mismatch | Out-Null
try {
    [System.IO.File]::WriteAllText((Join-Path $mismatch 'version.props'), "<Project><PropertyGroup><Version>9.9.9</Version><VersionSuffix>preview.1</VersionSuffix></PropertyGroup></Project>")
    [System.IO.File]::WriteAllText((Join-Path $mismatch 'Cargo.toml'), "version = `"0.1.0`"`r`nsuffix = `"preview.1`"`r`n")
    Assert-Throws { Assert-VeilCargoVersionMatches (Join-Path $mismatch 'version.props') (Join-Path $mismatch 'Cargo.toml') } 'does not match'
} finally {
    Remove-Item -LiteralPath $mismatch -Recurse -Force
}

# Execute the real publisher with only Git/GitHub transport replaced.
function Get-Command {
    param([string]$Name, $ErrorAction)
    if ($Name -ne 'gh') { throw "Unexpected command lookup: $Name" }
    [pscustomobject]@{ Source = 'Invoke-TestReleaseGh' }
}
function Invoke-TestReleaseGh {
    $global:VeilTestPublishArgs = @($args)
    $global:LASTEXITCODE = 0
}
$publishFixture = Join-Path ([System.IO.Path]::GetTempPath()) ("veil-publish-" + [guid]::NewGuid().ToString('n'))
$initialLocation = Get-Location
New-Item -ItemType Directory -Path (Join-Path $publishFixture 'src'), (Join-Path $publishFixture 'installer/dist') | Out-Null
try {
    foreach ($name in @('release.ps1', 'Get-VeilVersion.ps1', 'Test-VeilPublishedRelease.ps1', 'release-notes.template.md')) {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $publishFixture "installer/$name")
    }
    $global:VeilTestPublishing = $true
    foreach ($suffix in @('', 'preview.1')) {
        $suffixXml = if ($suffix) { "<VersionSuffix>$suffix</VersionSuffix>" } else { '' }
        Set-Content -LiteralPath (Join-Path $publishFixture 'src/version.props') -Value "<Project><PropertyGroup><Version>1.0.0</Version>$suffixXml</PropertyGroup></Project>"
        $global:VeilTestPublishTag = if ($suffix) { 'v1.0.0-preview.1' } else { 'v1.0.0' }
        $setupName = if ($suffix) { 'VeilSetup-1.0.0-preview.1-x64.exe' } else { 'VeilSetup-1.0.0-x64.exe' }
        $exe = Join-Path $publishFixture "installer/dist/$setupName"
        $sums = Join-Path $publishFixture 'installer/dist/SHA256SUMS.txt'
        $notes = Join-Path $publishFixture 'installer/dist/RELEASE-NOTES.md'
        Set-Content -LiteralPath $exe -Value 'test installer'
        Set-Content -LiteralPath $sums -Value 'test checksum'
        $global:VeilTestPublishArgs = @()
        & (Join-Path $publishFixture 'installer/release.ps1') -SkipBuild -SkipCleanCheck -Tag $global:VeilTestPublishTag | Out-Null
        $channelArgs = if ($suffix) { @('--prerelease', '--latest=false') } else { @('--latest') }
        $title = if ($suffix) { 'Veil 1.0.0-preview.1 preview' } else { 'Veil 1.0.0' }
        $expected = @('release', 'create', $global:VeilTestPublishTag, $exe, $sums) + $channelArgs + @('--title', $title, '--notes-file', $notes, '--target', 'abc123')
        if (($global:VeilTestPublishArgs -join "`n") -ne ($expected -join "`n")) {
            throw "Incorrect publication arguments for $global:VeilTestPublishTag : $($global:VeilTestPublishArgs -join ' ')"
        }
        $version = Get-VeilVersion -PropsPath (Join-Path $publishFixture 'src/version.props')
        if ($version.Tag -ne $global:VeilTestPublishTag -or $version.SetupFileName -ne $setupName) { throw 'Version labels do not match the publication channel.' }
        $notesText = Get-Content -LiteralPath $notes -Raw -Encoding utf8
        $expectedChannel = if ($suffix) { '预览版' } else { '正式版' }
        if ($notesText -notmatch [regex]::Escape("Veil $($version.Informational)（$expectedChannel）")) { throw 'Release notes must describe the actual version and channel.' }
        $cargoSuffix = if ($suffix) { "suffix = `"$suffix`"" } else { '' }
        Set-Content -LiteralPath (Join-Path $publishFixture 'src/Cargo.toml') -Value "version = `"1.0.0`"`n$cargoSuffix"
        Assert-VeilCargoVersionMatches (Join-Path $publishFixture 'src/version.props') (Join-Path $publishFixture 'src/Cargo.toml')
        Set-Content -LiteralPath (Join-Path $publishFixture 'src/Cargo.toml') -Value "version = `"1.0.0`"`nsuffix = `"other`""
        Assert-Throws { Assert-VeilCargoVersionMatches (Join-Path $publishFixture 'src/version.props') (Join-Path $publishFixture 'src/Cargo.toml') } 'suffix'
    }
} finally {
    $global:VeilTestPublishing = $false
    Set-Location $initialLocation
    $fixtureAbsolute = [IO.Path]::GetFullPath($publishFixture)
    $tempAbsolute = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if (-not $fixtureAbsolute.StartsWith($tempAbsolute, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test fixture cleanup path.' }
    Remove-Item -LiteralPath $fixtureAbsolute -Recurse -Force
}
Write-Output 'Version sources match.'

Write-Output 'Release checks passed: preflight, channels, publication arguments, and version consistency.'
