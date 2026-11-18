#requires -Version 7.2
# Exercise both entry points without credentials or network access.
$ErrorActionPreference = 'Stop'
$work = Join-Path ([IO.Path]::GetTempPath()) ('genshin-go-tickets-' + [guid]::NewGuid())
$root = Join-Path $work 'launcher-tickets'
$null = New-Item -ItemType Directory -Path $root -Force

function Assert-Equal($actual, $expected, $description) {
    if ($actual -cne $expected) { throw "$description (expected '$expected', got '$actual')" }
}
function Get-Argument($items, $name) {
    $items[[array]::IndexOf($items,$name) + 1]
}
function gh {
    $global:LASTEXITCODE = 0
    if ($args[0] -eq 'api') {
        $path = $args | Where-Object { $_ -like 'repos/*' }
        if ($path -like '*?state=all*') {
            foreach ($issue in $remote.Values) { ConvertTo-Json -Compress $issue }
        } elseif ($path -like '*/sub_issues*') {
            $remote.Keys | ForEach-Object { [string]$_ }
        } elseif ($path -match '/issues/(\d+)/dependencies/blocked_by') {
            $number = [int]$Matches[1]
            if ($args -contains 'POST') {
                $blocked[$number] = [int](Get-Argument $args '-F').Split('=')[1] - 100
            } elseif ($blocked.ContainsKey($number)) {
                $dep = $blocked[$number]
                ConvertTo-Json -Compress @{number=$dep; id=100+$dep}
            }
        } elseif ($path -match '/issues/(\d+)$') { 100 + [int]$Matches[1] }
        else { throw "Unexpected read: $args" }
    } elseif ($args[1] -eq 'edit') {
        $issue = $remote[[int]$args[2]]
        if ($args -contains '--title') { $issue.title = Get-Argument $args '--title' }
        if ($args -contains '--body-file') { $issue.body = Get-Content -Raw (Get-Argument $args '--body-file') }
    } elseif ($args[1] -eq 'create') {
        if ($remote.ContainsKey(8)) { throw 'Duplicate creation' }
        $remote[8] = @{ number=8; id=108; state='open'; title=(Get-Argument $args '--title'); labels=@('ready-for-agent'); body=(Get-Content -Raw (Get-Argument $args '--body-file')) }
        throw 'Simulated interruption after creation'
    } else { throw "Unexpected write: $args" }
}
try {
    foreach ($name in 'launcher-ticket-sync','publish-launcher-tickets','verify-launcher-tickets') {
        Copy-Item -LiteralPath "$PSScriptRoot/$name.ps1" -Destination $work
    }
    $remote = @{
        4 = @{number=4; id=104; title='[T01] old'; body="- [x] saved"; state='closed'; state_reason='completed'; labels=@()}
        7 = @{number=7; id=107; title='[T02] retired'; body='original'; state='closed'; state_reason='not_planned'; labels=@('wontfix')}
    }
    $blocked = @{}
    $plan = @(
        @{id=1; tag='T01'; title='[T01] renamed'; blockers=@(); bodyFile='T01.md'}
        @{id=2; tag='T02'; title='[T02] retired'; status='cancelled'; blockers=@(); bodyFile='T02.md'}
        @{id=3; tag='T03'; title='[T03] new'; blockers=@(1); bodyFile='T03.md'}
    )
    $mapPath = Join-Path $root 'issue-map.json'
    '{"T01":4,"T02":7}' | Set-Content $mapPath
    $original = Get-Content -Raw $mapPath
    ConvertTo-Json -Depth 4 $plan | Set-Content "$root/publication-plan.json"
    "- [ ] saved`nmore" | Set-Content "$root/T01.md"
    'cancelled' | Set-Content "$root/T02.md"
    'new' | Set-Content "$root/T03.md"
    & "$work/publish-launcher-tickets.ps1"
    Assert-Equal $remote.Count 2 'Preview is read-only'
    Assert-Equal $remote[4].title '[T01] old' 'Preview preserves titles'
    try {
        & "$work/publish-launcher-tickets.ps1" -Apply
        throw 'Expected interruption'
    } catch {
        if ($_.Exception.Message -notlike '*Simulated interruption after creation*') { throw }
    }
    Assert-Equal (Get-Content -Raw $mapPath) $original 'Interrupted run preserves the full mapping'
    & "$work/publish-launcher-tickets.ps1" -Apply
    Assert-Equal $remote.Count 3 'Retry reuses the created issue'
    Assert-Equal $remote[4].title '[T01] renamed' 'Rename retains issue number'
    Assert-Equal $remote[4].body.TrimEnd() "- [x] saved`nmore" 'Progress survives text edits'
    Assert-Equal $blocked[8] 4 'Current-plan dependency repaired on retry'
    & "$work/verify-launcher-tickets.ps1"
    $blocked.Remove(8)
    try {
        & "$work/verify-launcher-tickets.ps1"
        throw 'Expected drift detection'
    } catch {
        if ($_.Exception.Message -notlike '*differ from the current plan*') { throw }
    }
    Write-Output 'Offline ticket synchronization checks passed.'
} finally {
    # Clean only the unique directory created by this test.
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    $target = (Resolve-Path -LiteralPath $work).Path
    if ([IO.Path]::GetDirectoryName($target) -eq $tempRoot.TrimEnd('\','/')) {
        Remove-Item -LiteralPath $target -Recurse -Force
    }
}
