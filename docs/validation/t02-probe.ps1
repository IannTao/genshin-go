# Requests an unauthorised QR and prints only protocol metadata, never tickets or tokens.
$ErrorActionPreference = 'Stop'
$device = [guid]::NewGuid().ToString()
$base = 'https://hk4e-sdk.mihoyo.com/hk4e_cn/combo/panda/qrcode'
$body = @{ app_id = '4'; device = $device } | ConvertTo-Json -Compress
$created = Invoke-RestMethod -Method Post -Uri "$base/fetch" -ContentType 'application/json' -Body $body -TimeoutSec 20
if ($created.retcode -ne 0) { throw "QR creation failed: $($created.retcode)" }
$uri = [uri]$created.data.url
if ($uri.Scheme -ne 'https' -or $uri.Host -ne 'user.mihoyo.com') { throw 'Unexpected QR host' }
$ticketPair = $uri.Query.TrimStart('?').Split('&') | Where-Object { $_ -like 'ticket=*' }
if (-not $ticketPair) { throw 'Missing QR ticket' }
$body = @{ app_id = '4'; device = $device; ticket = $ticketPair.Substring(7) } | ConvertTo-Json -Compress
$queried = Invoke-RestMethod -Method Post -Uri "$base/query" -ContentType 'application/json' -Body $body -TimeoutSec 20
[pscustomobject]@{ phase = 'unauthorised-qr'; create_retcode = $created.retcode; query_retcode = $queried.retcode; state = $queried.data.stat; host = $uri.Host; path = $uri.AbsolutePath } | ConvertTo-Json

# An intentionally fictitious account and token: a negative response is not proof
# that the endpoint will accept a real, freshly confirmed authorization.
$invalid = Invoke-RestMethod -Method Post -Uri 'https://api-takumi.mihoyo.com/account/ma-cn-session/app/getTokenByGameToken' -ContentType 'application/json' -Headers @{ 'x-rpc-app_id' = 'bll8iq97cem8' } -Body '{"account_id":1,"game_token":"invalid-probe-token"}' -TimeoutSec 20
[pscustomobject]@{ phase = 'invalid-authorization'; retcode = $invalid.retcode; message = $invalid.message } | ConvertTo-Json
