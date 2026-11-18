#requires -Version 7.2
$ErrorActionPreference = 'Stop'
$repository = 'IannTao/genshin-go'
$parentNumber = 1

function Invoke-GitHub {
    param([string[]]$Arguments)
    $output = & gh @Arguments
    if ($LASTEXITCODE -ne 0) { throw "GitHub command failed: gh $($Arguments -join ' ')" }
    $output
}

function Read-IssueRows([string]$endpoint, [string]$filter) {
    Invoke-GitHub -Arguments @('api', '--paginate', $endpoint, '--jq', $filter) |
        ForEach-Object { ConvertFrom-Json -InputObject $_ -AsHashtable }
}

function Get-TrackerSnapshot {
    $endpoint = "repos/$repository/issues"
    $issues = @(Read-IssueRows "${endpoint}?state=all&per_page=100" '.[] | select(.pull_request == null) | {number,id,title,body,state,state_reason,labels:[.labels[].name]} | @json')
    $children = @(Read-IssueRows "$endpoint/$parentNumber/sub_issues?per_page=100" '.[] | .number | @json')
    @{ Issues = $issues; Children = $children }
}

function Read-Blockers([int]$number) {
    Read-IssueRows "repos/$repository/issues/$number/dependencies/blocked_by?per_page=100" '.[] | {number,id} | @json'
}

function Save-IssueMap($map, [string]$path) {
    $temp = "$path.$([guid]::NewGuid()).tmp"
    try {
        [IO.File]::WriteAllText($temp, (ConvertTo-Json $map) + [Environment]::NewLine)
        [IO.File]::Move($temp, $path, $true)
    } finally {
        if ([IO.File]::Exists($temp)) { [IO.File]::Delete($temp) }
    }
}

function Get-DesiredBody([string]$local, [string]$remote) {
    # Completion belongs to GitHub. Preserve checked criteria that still exist in the draft.
    $checked = @($remote.Replace("`r`n","`n").Split("`n") | Where-Object { $_ -cmatch '^- \[[xX]\] ' } |
        ForEach-Object { $_.Substring(6) })
    ($local.Replace("`r`n","`n").TrimEnd().Split("`n") | ForEach-Object {
        if ($_.StartsWith('- [ ] ') -and $_.Substring(6) -cin $checked) { '- [x] ' + $_.Substring(6) }
        else { $_ }
    }) -join "`n"
}

function Get-TicketChanges([string]$root, $map, $snapshot) {
    $plan = @(Get-Content -Raw (Join-Path $root 'publication-plan.json') | ConvertFrom-Json)
    $byId = @{}
    $tags = @{}
    $assigned = @{}
    foreach ($tag in $map.Keys) {
        $number = [int]$map[$tag]
        if ($number -le 1 -or $assigned.ContainsKey($number)) { throw "Invalid mapping: $tag -> #$number" }
        $assigned[$number] = $tag
    }
    foreach ($ticket in $plan) {
        $tag = $ticket.tag
        if ($tags.ContainsKey($tag) -or $byId.ContainsKey($ticket.id) -or $tag -cne ('T{0:D2}' -f $ticket.id)) { throw "Invalid ticket identity: $tag" }
        if ($ticket.status -and $ticket.status -ne 'cancelled') { throw "Unknown status for $tag" }
        $blockedBy = @(foreach ($id in $ticket.blockers) {
            $prior = $byId[$id]
            if (!$prior -or $prior.status -eq 'cancelled') { throw "$tag has an invalid or out-of-order blocker: $id" }
            $prior.tag
        })
        $tags[$tag] = $true
        $byId[$ticket.id] = $ticket
        $issue = $null
        $changes = [Collections.Generic.List[string]]::new()
        if ($map.Contains($tag)) {
            $issue = $snapshot.Issues | Where-Object number -eq $map[$tag]
            if (!$issue) { throw "Mapped issue missing: $tag #$($map[$tag])" }
        } else {
            # Recover a creation interrupted before the local mapping was committed.
            $matches = @($snapshot.Issues | Where-Object { $_.title.StartsWith("[$tag] ") })
            if ($matches.Count -gt 1) { throw "Ambiguous issue tag: $tag" }
            if ($matches.Count) {
                $issue = $matches[0]
                if ($assigned.ContainsKey([int]$issue.number)) { throw "Issue #$($issue.number) already mapped" }
                $map[$tag] = $issue.number
                $assigned[[int]$issue.number] = $tag
                $changes.Add('map')
            } else {
                if ($ticket.status -eq 'cancelled') { throw "Missing cancelled issue: $tag" }
                $changes.Add('create')
            }
        }
        $local = Get-Content -Raw -LiteralPath (Join-Path $root $ticket.bodyFile)
        $body = Get-DesiredBody $local ([string]$issue.body)
        $actual = @()
        if ($issue) {
            if ($issue.number -notin $snapshot.Children) { $changes.Add('parent') }
            if ($issue.title -cne $ticket.title) { $changes.Add('title') }
            if ($ticket.status -eq 'cancelled') {
                if ($issue.state_reason -ne 'not_planned' -or $issue.state -ne 'closed') { $changes.Add('cancel') }
                if ('wontfix' -notin $issue.labels -or 'ready-for-agent' -in $issue.labels) { $changes.Add('cancel-labels') }
            } else {
                if ($issue.state -eq 'closed' -and $issue.state_reason -ne 'completed') { throw "$tag closed unexpectedly: $($issue.state_reason)" }
                $openTriage = @('needs-triage','needs-info','ready-for-agent','ready-for-human')
                if ($issue.state -eq 'open' -and !@($issue.labels | Where-Object { $_ -in $openTriage }).Count) { $changes.Add('ready-label') }
                if ($body -cne ([string]$issue.body).Replace("`r`n","`n").TrimEnd()) { $changes.Add('body') }
            }
            $actual = @(Read-Blockers $issue.number)
            $expected = @($blockedBy | ForEach-Object { $map[$_] })
            if ((@($actual | ForEach-Object number | Sort-Object) -join ',') -ne (@($expected | Sort-Object) -join ',') -or $expected -contains $null) { $changes.Add('dependencies') }
        }
        [pscustomobject]@{ Ticket=$ticket; Issue=$issue; Body=$body; BlockerTags=$blockedBy; Actual=$actual; Changes=$changes }
    }
    foreach ($tag in $map.Keys) {
        if (!$tags.ContainsKey($tag)) { throw "$tag is mapped but missing from the plan. Mark cancelled tasks explicitly." }
    }
}

function Show-TicketChanges($rows) {
    foreach ($row in $rows) {
        if (!$row.Changes.Count) { continue }
        Write-Host "$($row.Ticket.tag) #$($row.Issue.number) [$($row.Changes -join ', ')]"
        if ('title' -in $row.Changes -or 'create' -in $row.Changes) { Write-Host "  $($row.Issue.title) -> $($row.Ticket.title)" }
        if ('dependencies' -in $row.Changes -or 'create' -in $row.Changes) {
            Write-Host "  Blocked by: #$(@($row.Actual | ForEach-Object number) -join ', #') -> $($row.BlockerTags -join ', ')"
        }
        if ('body' -in $row.Changes -or 'create' -in $row.Changes) {
            $before = ([string]$row.Issue.body).Replace("`r`n","`n").Split("`n")
            Compare-Object $before $row.Body.Split("`n") -CaseSensitive -SyncWindow 0 |
                ForEach-Object { Write-Host "  $($_.SideIndicator) $($_.InputObject)" }
        }
    }
}
