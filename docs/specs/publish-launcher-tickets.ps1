#requires -Version 7.2
param([switch]$Apply)
. "$PSScriptRoot/launcher-ticket-sync.ps1"
$root = Join-Path $PSScriptRoot 'launcher-tickets'
$mapPath = Join-Path $root 'issue-map.json'
# Serialize local writers. The OS releases the handle if a run is interrupted.
$mutex = [Threading.Mutex]::new($false, 'GenshinGoLauncherIssuePublisher')
try {
    if ($Apply) {
        try { $locked = $mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $locked = $true }
        if (!$locked) { throw 'Another publisher is running.' }
    }
    $map = Get-Content -Raw $mapPath | ConvertFrom-Json -AsHashtable
    $snapshot = Get-TrackerSnapshot
    $rows = @(Get-TicketChanges $root $map $snapshot)
    Show-TicketChanges $rows
    if (!$Apply) { Write-Host 'Preview only. Use -Apply to publish these changes.'; return }
    foreach ($row in $rows) {
        $changes = $row.Changes
        if (!$changes.Count) { continue }
        $tag = $row.Ticket.tag
        $number = $map[$tag]
        $file = [IO.Path]::GetTempFileName()
        try {
            [IO.File]::WriteAllText($file, $row.Body)
            if ('create' -in $changes) {
                $argsCreate = @('issue','create','--repo',$repository,'--title',$row.Ticket.title,'--body-file',$file,'--label','ready-for-agent','--parent',"$parentNumber")
                $url = Invoke-GitHub -Arguments $argsCreate | Select-Object -Last 1
                if ($url -notmatch '/issues/(\d+)$') { throw "Unexpected create result: $url" }
                $number = [int]$Matches[1]
                $map[$tag] = $number
            }
            if ('parent' -in $changes) {
                Invoke-GitHub -Arguments @('api','--silent','-X','POST',"repos/$repository/issues/$parentNumber/sub_issues",'-F',"sub_issue_id=$($row.Issue.id)")
            }
            $options = @()
            if ('title' -in $changes) { $options += @('--title',$row.Ticket.title) }
            if ('body' -in $changes) { $options += @('--body-file',$file) }
            if ('ready-label' -in $changes) { $options += @('--add-label','ready-for-agent') }
            if ('cancel-labels' -in $changes) {
                $options += @('--add-label','wontfix')
                if ('ready-for-agent' -in $row.Issue.labels) { $options += @('--remove-label','ready-for-agent') }
            }
            if ($options.Count) { Invoke-GitHub -Arguments (@('issue','edit',"$number",'--repo',$repository) + $options) }
            if ('dependencies' -in $changes -or 'create' -in $changes) {
                $wanted = @($row.BlockerTags | ForEach-Object { $map[$_] })
                $endpoint = "repos/$repository/issues/$number/dependencies/blocked_by"
                foreach ($blocker in $wanted) {
                    if ($blocker -notin @($row.Actual | ForEach-Object number)) {
                        $id = Invoke-GitHub -Arguments @('api',"repos/$repository/issues/$blocker",'--jq','.id')
                        Invoke-GitHub -Arguments @('api','--silent','-X','POST',$endpoint,'-F',"issue_id=$id")
                    }
                }
                foreach ($edge in $row.Actual) {
                    if ($edge.number -notin $wanted) { Invoke-GitHub -Arguments @('api','--silent','-X','DELETE',"$endpoint/$($edge.id)") }
                }
            }
            if ('cancel' -in $changes) {
                Invoke-GitHub -Arguments @('api','--silent','-X','PATCH',"repos/$repository/issues/$number",'-f','state=closed','-f','state_reason=not_planned')
            }
        } finally { [IO.File]::Delete($file) }
    }
    if (@($rows | Where-Object { 'map' -in $_.Changes -or 'create' -in $_.Changes }).Count) { Save-IssueMap $map $mapPath }
} finally {
    if ($locked) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
