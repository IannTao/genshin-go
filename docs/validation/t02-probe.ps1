# Creates an unauthorised passport QR and prints protocol metadata only.
$ErrorActionPreference = 'Stop'
$device = [guid]::NewGuid().ToString('N')
$headers = @{ 'x-rpc-app_id' = 'bll8iq97cem8'; 'x-rpc-device_id' = $device }
$base = 'https://passport-api.miyoushe.com/account/ma-cn-passport/web'
$created = Invoke-RestMethod -Method Post -Uri "$base/createQRLogin" -Headers $headers -ContentType 'application/json' -Body '{}' -TimeoutSec 20
if ($created.retcode -ne 0 -or -not $created.data.ticket) { throw "QR creation failed: $($created.retcode)" }
$uri = [uri]$created.data.url
if ($uri.Scheme -ne 'https' -or $uri.Host -ne 'user.mihoyo.com') { throw 'Unexpected QR host' }
$body = @{ ticket = $created.data.ticket } | ConvertTo-Json -Compress
$queried = Invoke-RestMethod -Method Post -Uri "$base/queryQRLoginStatus" -Headers $headers -ContentType 'application/json' -Body $body -TimeoutSec 20
[pscustomobject]@{ create_retcode = $created.retcode; query_retcode = $queried.retcode; state = $queried.data.status; host = $uri.Host; path = $uri.AbsolutePath } | ConvertTo-Json
