#requires -Version 7.2
$ErrorActionPreference = 'Stop'

$validationDirectory = Join-Path ([IO.Path]::GetTempPath()) ('genshin-go-passport-' + [guid]::NewGuid())
$previousLocalAppData = $env:LOCALAPPDATA
$previousValidationLocalAppData = $env:GENSHIN_GO_VALIDATION_LOCALAPPDATA
Push-Location (Join-Path $PSScriptRoot '../..')
try {
    $null = New-Item -ItemType Directory -Path $validationDirectory
    $env:LOCALAPPDATA = $validationDirectory
    $env:GENSHIN_GO_VALIDATION_LOCALAPPDATA = $validationDirectory
    Write-Host "验证目录 $validationDirectory"
    Write-Host '启动后请在两分钟内用米游社扫码并确认。'
    Get-Content (Join-Path $PSScriptRoot 't02-windows-save.js') -Raw |
        nsh test nsh.json --target windows
    if ($LASTEXITCODE -ne 0) { throw "nsh test nsh.json --target windows 失败（退出码 $LASTEXITCODE）" }

    Write-Host '重开启动器并检查已保存账号'
    Get-Content (Join-Path $PSScriptRoot 't02-windows-restore.js') -Raw |
        nsh test nsh.json --target windows
    if ($LASTEXITCODE -ne 0) { throw "nsh test nsh.json --target windows 失败（退出码 $LASTEXITCODE）" }

    Write-Host '验证 DPAPI 解密后仍能查询到相同的游戏 UID'
    cargo test --locked --lib account_store::tests::restored_passport_authorization_has_paired_cookie_fields -- --ignored
    if ($LASTEXITCODE -ne 0) { throw "cargo test --locked --lib account_store::tests:: -- --ignored --nocapture 失败（退出码 $LASTEXITCODE）" }
} finally {
    $env:LOCALAPPDATA = $previousLocalAppData
    $env:GENSHIN_GO_VALIDATION_LOCALAPPDATA = $previousValidationLocalAppData
    Pop-Location
}
