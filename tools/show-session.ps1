# 打印最近一次产品会话记录。原始拓扑字节不要提交。
param(
    [string] $Session
)

$root = Join-Path $env:LOCALAPPDATA "Veil"
if (-not (Test-Path $root)) {
    Write-Error "没有 $root"
    exit 1
}

$dir = if ($Session) {
    if (Test-Path $Session) { $Session } else { Join-Path $root $Session }
} else {
    Get-ChildItem $root -Directory -Filter "session-*" |
        Sort-Object LastWriteTime |
        Select-Object -Last 1 |
        ForEach-Object { $_.FullName }
}

if (-not $dir -or -not (Test-Path $dir)) {
    Write-Error "找不到会话目录。"
    exit 1
}

Write-Output "SESSION $dir"
foreach ($name in @("ready.json", "arm.json", "intent.json", "heartbeat.json", "result.json")) {
    $path = Join-Path $dir $name
    if (Test-Path $path) {
        Write-Output "----- $name -----"
        Get-Content -LiteralPath $path -Raw
    }
}

$events = Join-Path $dir "events.jsonl"
if (Test-Path $events) {
    Write-Output "----- events.jsonl -----"
    Get-Content -LiteralPath $events
} else {
    Write-Output "----- events.jsonl -----"
    Write-Output "(本会话没有 events.jsonl。旧构建只覆盖 heartbeat/result。)"
}
