function Get-VeilVersion {
    param(
        [string]$PropsPath
    )

    if (-not $PropsPath) {
        $installerDir = $PSScriptRoot
        if (-not $installerDir) {
            $installerDir = Split-Path -Parent $MyInvocation.MyCommand.Path
        }
        $PropsPath = Join-Path (Split-Path -Parent $installerDir) "src\version.props"
    }

    if (-not (Test-Path -LiteralPath $PropsPath)) {
        throw "Version file not found: $PropsPath"
    }

    $text = Get-Content -LiteralPath $PropsPath -Raw -Encoding UTF8
    if ($text -notmatch '<Version>([^<]+)</Version>') {
        throw "Version file is missing <Version>"
    }
    $version = $Matches[1].Trim()
    if ($version -notmatch '^\d+\.\d+\.\d+$') {
        throw "Version must be Major.Minor.Build, got: $version"
    }

    $suffix = $null
    if ($text -match '<VersionSuffix>([^<]+)</VersionSuffix>') {
        $suffix = $Matches[1].Trim()
        if (-not $suffix) { $suffix = $null }
    }

    $informational = $version
    if ($suffix) {
        $informational = "$version-$suffix"
    }

    [pscustomobject]@{
        Version        = $version
        VersionSuffix  = $suffix
        Informational  = $informational
        Tag            = "v$informational"
        FileLabel      = $informational
        SetupFileName  = "VeilSetup-$informational-x64.exe"
    }
}

function Get-VeilReleaseNotes {
    param(
        [Parameter(Mandatory = $true)]
        $VersionInfo
    )

    $template = Join-Path $PSScriptRoot "release-notes.template.md"
    if (-not (Test-Path -LiteralPath $template)) {
        throw "Missing $template"
    }

    $text = Get-Content -LiteralPath $template -Raw -Encoding UTF8
    $text = $text.Replace("{{INFORMATIONAL}}", [string]$VersionInfo.Informational)
    $text = $text.Replace("{{SETUP_FILE}}", [string]$VersionInfo.SetupFileName)
    return $text
}
