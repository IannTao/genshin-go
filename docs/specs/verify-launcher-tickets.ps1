#requires -Version 7.2
. "$PSScriptRoot/launcher-ticket-sync.ps1"
$root = Join-Path $PSScriptRoot 'launcher-tickets'
$map = Get-Content -Raw (Join-Path $root 'issue-map.json') | ConvertFrom-Json -AsHashtable
$rows = @(Get-TicketChanges $root $map (Get-TrackerSnapshot))
Show-TicketChanges $rows
$drift = @($rows | Where-Object { $_.Changes.Count })
if ($drift.Count) { throw "$($drift.Count) ticket(s) differ from the current plan." }
Write-Output "Verified $($rows.Count) tracked issues against the current plan."
