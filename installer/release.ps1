param(
    [string]$Tag,
    [switch]$SkipBuild,
    [switch]$SkipCleanCheck,
    [switch]$SkipTest
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "Get-VeilVersion.ps1")
$version = Get-VeilVersion
$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repo

if (-not $Tag) {
    $Tag = $version.Tag
}
elseif ($Tag -ne $version.Tag) {
    throw ("Tag {0} does not match Directory.Build.props tag {1}." -f $Tag, $version.Tag)
}

if (-not $SkipCleanCheck) {
    $status = @(git status --porcelain)
    if ($status.Count -gt 0) {
        throw "Working tree is dirty. Commit or stash first, or pass -SkipCleanCheck in CI."
    }
}

$ps = Join-Path $env:WINDIR "System32\WindowsPowerShell\v1.0\powershell.exe"
$dotnet = Join-Path $env:ProgramFiles "dotnet\dotnet.exe"
if (-not (Test-Path $dotnet)) { throw "dotnet SDK not found: $dotnet" }

if (-not $SkipBuild) {
    if (-not $SkipTest) {
        & $dotnet test (Join-Path $repo "src\Veil.sln") -p:Platform=x64 --configuration Release
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

$head = (git rev-parse HEAD).Trim()
$existingTag = @(git tag -l $Tag)
if ($existingTag.Count -gt 0) {
    $tagged = (git rev-parse ("{0}^{{}}" -f $Tag)).Trim()
    if ($tagged -ne $head) {
        throw ("Local tag {0} points to {1}, HEAD is {2}" -f $Tag, $tagged, $head)
    }
}
else {
    git tag $Tag
}

$notes = Join-Path $dist "RELEASE-NOTES.md"
Set-Content -LiteralPath $notes -Encoding UTF8 -Value (Get-VeilReleaseNotes -VersionInfo $version)

$gh = Get-Command gh -ErrorAction SilentlyContinue
if (-not $gh) {
    throw "gh not found. Install GitHub CLI, or use pack.ps1 only."
}

& $gh.Source release create $Tag $exe $sums --prerelease --title ("Veil {0} preview" -f $version.Informational) --notes-file $notes --target $head
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Output ("created prerelease {0} from {1}; tag was not pushed with git push --tags." -f $Tag, $head)
