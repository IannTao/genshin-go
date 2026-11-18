#requires -Version 7.2
$ErrorActionPreference = 'Stop'

Push-Location (Join-Path $PSScriptRoot '../..')
try {
    cargo test --locked --lib account_service::tests::live_unconfirmed_qr -- --ignored --exact
    if ($LASTEXITCODE -ne 0) { throw "cargo test --locked --lib account_service::tests::live_unconfirmed_qr -- --ignored --exact 失败（退出码 $LASTEXITCODE）" }
} finally {
    Pop-Location
}
