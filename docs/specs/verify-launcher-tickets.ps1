$ErrorActionPreference = 'Stop'
$ticketRoot = Join-Path $PSScriptRoot 'launcher-tickets'
$plan = @(Get-Content -Raw (Join-Path $ticketRoot 'publication-plan.json') | ConvertFrom-Json)
$published = @(Get-Content -Raw (Join-Path $ticketRoot 'published.json') | ConvertFrom-Json)
if ($published.Count -ne 28) { throw "Expected 28 tickets, found $($published.Count)" }
$remoteText = gh issue list --repo IannTao/genshin-go --state all --limit 200 --json number,title,body,labels,state,url
if ($LASTEXITCODE -ne 0) { throw 'Cannot read issues' }
$remote = @($remoteText | ConvertFrom-Json)
$childrenText = gh api repos/IannTao/genshin-go/issues/1/sub_issues --paginate --jq '.[].number'
if ($LASTEXITCODE -ne 0) { throw 'Cannot read parent children' }
$children = @($childrenText | ForEach-Object { [int]$_ })
$edgeCount = 0
foreach ($ticket in $plan) {
    $record = @($published | Where-Object tag -eq $ticket.tag)[0]
    $issue = @($remote | Where-Object number -eq $record.number)[0]
    if (-not $issue -or $issue.title -cne $ticket.title) { throw "Missing/mismatched $($ticket.tag)" }
    $localBody = Get-Content -Raw (Join-Path $ticketRoot $ticket.bodyFile)
    if ($issue.body.Replace("`r`n","`n").TrimEnd() -cne $localBody.Replace("`r`n","`n").TrimEnd()) { throw "Body mismatch: $($ticket.tag)" }
    if ($issue.state -ne 'OPEN' -or 'ready-for-agent' -notin @($issue.labels.name)) { throw "State or label mismatch: $($ticket.tag)" }
    if ($record.number -notin $children) { throw "Parent mismatch: $($ticket.tag)" }
    $dependencyText = gh api "repos/IannTao/genshin-go/issues/$($record.number)/dependencies/blocked_by" --paginate --jq '.[].number'
    if ($LASTEXITCODE -ne 0) { throw "Cannot read dependencies: $($ticket.tag)" }
    $actual = @($dependencyText | Where-Object { $_ -match '^\d+$' } | ForEach-Object { [int]$_ } | Sort-Object)
    $expected = @($record.blockers | Sort-Object)
    if (($actual -join ',') -ne ($expected -join ',')) { throw "Dependency mismatch: $($ticket.tag)" }
    $edgeCount += $actual.Count
    Write-Output "Verified $($ticket.tag): #$($record.number)"
}
$parent = @($remote | Where-Object number -eq 1)[0]
$spec = Get-Content -Raw (Join-Path $PSScriptRoot 'launcher-phase-one.md')
if ($parent.state -ne 'OPEN' -or $parent.body.Replace("`r`n","`n").TrimEnd() -cne $spec.Replace("`r`n","`n").TrimEnd()) { throw 'Parent body/state changed' }
[pscustomobject]@{tickets=$published.Count;nativeBlockingEdges=$edgeCount;allChildrenLinked=$true;bodiesAndLabelsVerified=$true;parentBodyAndStateUnchanged=$true} | ConvertTo-Json | Tee-Object -FilePath (Join-Path $ticketRoot 'verification.json')

