$ErrorActionPreference = 'Stop'
$ticketRoot = Join-Path $PSScriptRoot 'launcher-tickets'
$plan = @(Get-Content -Raw (Join-Path $ticketRoot 'publication-plan.json') | ConvertFrom-Json)
$existingText = gh issue list --repo IannTao/genshin-go --state all --limit 200 --json number,title,url
if ($LASTEXITCODE -ne 0) { throw 'Cannot read existing issues' }
$existing = @($existingText | ConvertFrom-Json)
$mapping = @{}
$results = [System.Collections.Generic.List[object]]::new()
foreach ($ticket in $plan) {
    $matches = @($existing | Where-Object { $_.title -ceq $ticket.title })
    if ($matches.Count -gt 1) { throw "Duplicate titles: $($ticket.title)" }
    $blockers = @($ticket.blockers | ForEach-Object {
        $key = [string]$_
        if (-not $mapping.ContainsKey($key)) { throw "Missing blocker $key" }
        $mapping[$key]
    })
    if ($matches.Count -eq 1) {
        $number = [int]$matches[0].number
        $url = $matches[0].url
        Write-Output "Existing $($ticket.tag): #$number"
    } else {
        $createArgs = @('issue','create','--repo','IannTao/genshin-go','--title',$ticket.title,'--body-file',(Join-Path $ticketRoot $ticket.bodyFile),'--label','ready-for-agent','--parent','1')
        if ($blockers.Count -gt 0) { $createArgs += @('--blocked-by',($blockers -join ',')) }
        $output = & gh @createArgs
        if ($LASTEXITCODE -ne 0) { throw "Create failed for $($ticket.tag); inspect remote before retry" }
        $url = ($output | Select-Object -Last 1).Trim()
        if ($url -notmatch '/issues/(\d+)$') { throw "Unexpected create result: $url" }
        $number = [int]$Matches[1]
        Write-Output "Created $($ticket.tag): #$number; blockers: $($blockers -join ',')"
    }
    $mapping[[string]$ticket.id] = $number
    $results.Add([pscustomobject]@{tag=$ticket.tag;number=$number;url=$url;blockers=$blockers})
    $results.ToArray() | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $ticketRoot 'published.json')
}
Write-Output "Published or found $($results.Count) tickets."

