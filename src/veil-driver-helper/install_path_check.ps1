$ErrorActionPreference = 'Stop'

try {
    $requested = $env:VEIL_INSTALL_DIR
    if ([string]::IsNullOrWhiteSpace($requested) -or
        -not [IO.Path]::IsPathRooted($requested) -or
        $requested.StartsWith('\\') -or
        $requested.Contains('..')) {
        throw '安装路径必须是本机固定磁盘上的绝对路径。'
    }
    $target = [IO.Path]::GetFullPath($requested).TrimEnd('\')
    $recorded = (Get-ItemProperty -LiteralPath 'HKLM:\Software\Veil' -Name InstallDir -ErrorAction SilentlyContinue).InstallDir
    if ([string]::IsNullOrWhiteSpace($recorded)) {
        $legacy = Join-Path $env:ProgramFiles 'Veil'
        if (Test-Path -LiteralPath (Join-Path $legacy 'Veil.App.exe')) { $recorded = $legacy }
    }
    if (-not [string]::IsNullOrWhiteSpace($recorded) -and
        -not $target.Equals([IO.Path]::GetFullPath($recorded).TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) {
        throw '已安装 Veil；升级必须沿用原目录。若要迁移，请先卸载。'
    }
    $root = [IO.Path]::GetPathRoot($target)
    if ($target -eq $root -or [string]::IsNullOrWhiteSpace($root)) {
        throw '不能将 Veil 安装在磁盘根目录。'
    }
    $drive = New-Object IO.DriveInfo($root)
    if ($drive.DriveType -ne [IO.DriveType]::Fixed -or -not $drive.IsReady) {
        throw '安装路径必须位于本机固定磁盘。'
    }
    $current = $target
    while (-not (Test-Path -LiteralPath $current -PathType Container)) {
        if (Test-Path -LiteralPath $current) { throw '安装路径包含非目录项。' }
        $parent = [IO.Directory]::GetParent($current)
        if ($null -eq $parent) { throw '找不到有效的安装目录。' }
        $current = $parent.FullName
    }
    if ($current -eq $root) {
        throw '请在受保护的现有目录下安装 Veil。'
    }
    $writeBits = [int64]0x000D0156 # write/create/delete/change-permissions/take-ownership
    while ($current -ne $root) {
        $existing = Get-Item -LiteralPath $current -Force
        if (($existing.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw '安装路径不能经过重解析点。'
        }
        $acl = Get-Acl -LiteralPath $current
        $owner = $acl.GetOwner([Security.Principal.SecurityIdentifier]).Value
        if ($owner -notin @('S-1-5-18', 'S-1-5-32-544') -and -not $owner.StartsWith('S-1-5-80-')) {
            throw "安装目录的现有父目录必须由系统或管理员拥有：$current"
        }
        foreach ($rule in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
            if ($rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow) { continue }
            if (($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly) -ne 0) { continue }
            $sid = $rule.IdentityReference.Value
            if ($sid -in @('S-1-5-18', 'S-1-5-32-544') -or $sid.StartsWith('S-1-5-80-')) { continue }
            if (([int64]$rule.FileSystemRights -band $writeBits) -ne 0) {
                throw "安装目录对普通用户可写：$current"
            }
        }
        $current = [IO.Directory]::GetParent($current).FullName
    }
    exit 0
}
catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
