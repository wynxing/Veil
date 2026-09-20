function Test-VeilPublishedRelease {
    param(
        [Parameter(Mandatory = $true)][string]$Tag,
        [Parameter(Mandatory = $true)][string]$SetupFileName,
        [Parameter(Mandatory = $true)][string]$Head
    )

    # List all pages so authentication/network failures cannot masquerade as 404.
    $json = & gh api 'repos/{owner}/{repo}/releases' --paginate --slurp
    if ($LASTEXITCODE -ne 0) { throw 'Failed to query GitHub releases.' }
    $pages = ($json -join "`n") | ConvertFrom-Json -ErrorAction Stop
    $existing = @($pages | ForEach-Object { $_ } | Where-Object { $_.tag_name -eq $Tag })
    if ($existing.Count -eq 0) { return $false }
    if ($existing.Count -ne 1) { throw "Ambiguous release for $Tag." }
    $release = $existing[0]

    $refs = @(& git ls-remote --exit-code origin "refs/tags/$Tag" "refs/tags/$Tag^{}")
    if ($LASTEXITCODE -ne 0 -or $refs.Count -eq 0) { throw "Cannot verify remote tag $Tag." }
    $peeled = @($refs | Where-Object { $_ -like '*^{}' })
    $ref = if ($peeled.Count -gt 0) { $peeled[0] } else { $refs[0] }
    $commit = ($ref -split '\s+')[0]
    if ($commit -ne $Head) { throw "Remote tag $Tag points to $commit, HEAD is $Head." }
    if ($release.draft) { throw "Release $Tag is a draft; inspect it before publishing." }
    foreach ($name in @($SetupFileName, 'SHA256SUMS.txt')) {
        $asset = @($release.assets | Where-Object { $_.name -eq $name -and $_.state -eq 'uploaded' -and $_.size -gt 0 })
        if ($asset.Count -ne 1) { throw "Release $Tag is incomplete: missing uploaded asset $name. Inspect it before retrying." }
    }
    # Preserve published bytes; do not silently replace an existing installer.
    return $true
}
