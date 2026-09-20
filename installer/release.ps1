param(
    [string]$Tag,
    [switch]$SkipBuild,
    [switch]$SkipCleanCheck,
    [switch]$SkipTest,
    [switch]$CheckOnly
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "Get-VeilVersion.ps1")
. (Join-Path $PSScriptRoot "Test-VeilPublishedRelease.ps1")
$version = Get-VeilVersion
$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repo

if (-not $Tag) {
    $Tag = $version.Tag
}
elseif ($Tag -ne $version.Tag) {
    throw ("Tag {0} does not match src/version.props tag {1}." -f $Tag, $version.Tag)
}

if (-not $SkipCleanCheck) {
    $status = @(git status --porcelain)
    if ($status.Count -gt 0) {
        throw "Working tree is dirty. Commit or stash first, or pass -SkipCleanCheck in CI."
    }
}

$head = (git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve HEAD.' }
$existingTag = @(git tag -l $Tag)
if ($LASTEXITCODE -ne 0) { throw 'Cannot query local tags.' }
if ($existingTag.Count -gt 0) {
    $tagged = (git rev-parse ("{0}^{{}}" -f $Tag)).Trim()
    if ($LASTEXITCODE -ne 0 -or $tagged -ne $head) {
        throw ("Local tag {0} points to {1}, HEAD is {2}" -f $Tag, $tagged, $head)
    }
}
$gh = Get-Command gh -ErrorAction Stop
$published = Test-VeilPublishedRelease -Tag $Tag -SetupFileName $version.SetupFileName -Head $head
if ($CheckOnly) {
    if ($env:GITHUB_OUTPUT) {
        "published=$($published.ToString().ToLowerInvariant())" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
    }
    Write-Output ("Release {0}: published={1}" -f $Tag, $published)
    return
}
if ($published) {
    Write-Output "Release $Tag already has its installer and checksums; keeping existing assets."
    return
}

$ps = Join-Path $env:WINDIR "System32\WindowsPowerShell\v1.0\powershell.exe"
$dotnet = Join-Path $env:ProgramFiles "dotnet\dotnet.exe"
if (-not (Test-Path $dotnet)) { throw "dotnet SDK not found: $dotnet" }

if (-not $SkipBuild) {
    if (-not $SkipTest) {
        cargo test --manifest-path (Join-Path $repo "src\Cargo.toml") --workspace --locked --release --target x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    & $ps -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "pack.ps1")
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$dist = Join-Path $PSScriptRoot "dist"
$exe = Join-Path $dist $version.SetupFileName
$sums = Join-Path $dist "SHA256SUMS.txt"
if (-not (Test-Path -LiteralPath $exe) -or -not (Test-Path -LiteralPath $sums)) {
    throw "Pack outputs are missing. Run installer/pack.ps1 first."
}

if ($existingTag.Count -eq 0) {
    git tag $Tag
    if ($LASTEXITCODE -ne 0) { throw "Cannot create local tag $Tag." }
}

$notes = Join-Path $dist "RELEASE-NOTES.md"
Set-Content -LiteralPath $notes -Encoding UTF8 -Value (Get-VeilReleaseNotes -VersionInfo $version)

& $gh.Source release create $Tag $exe $sums --prerelease --title ("Veil {0} preview" -f $version.Informational) --notes-file $notes --target $head
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Output ("created prerelease {0} from {1}; tag was not pushed with git push --tags." -f $Tag, $head)
