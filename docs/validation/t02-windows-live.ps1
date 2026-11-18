#requires -Version 7.2
$ErrorActionPreference = 'Stop'

$validationDirectory = Join-Path ([IO.Path]::GetTempPath()) ('genshin-go-passport-' + [guid]::NewGuid())
$previousLocalAppData = $env:LOCALAPPDATA
$previousPassportTrace = $env:GENSHIN_GO_TRACE_PASSPORT
Push-Location (Join-Path $PSScriptRoot '../..')
try {
    $null = New-Item -ItemType Directory -Path $validationDirectory
    $env:LOCALAPPDATA = $validationDirectory
    $env:GENSHIN_GO_TRACE_PASSPORT = '1'
    Write-Host "验证目录 $validationDirectory"
    Write-Host '启动后请在两分钟内用米游社扫码并确认。'
    Get-Content (Join-Path $PSScriptRoot 't02-windows-save.js') -Raw |
        nsh test nsh.json --target windows
    if ($LASTEXITCODE -ne 0) { throw "nsh test nsh.json --target windows 失败（退出码 $LASTEXITCODE）" }

    Write-Host '重开启动器并检查已保存账号'
    Get-Content (Join-Path $PSScriptRoot 't02-windows-restore.js') -Raw |
        nsh test nsh.json --target windows
    if ($LASTEXITCODE -ne 0) { throw "nsh test nsh.json --target windows 失败（退出码 $LASTEXITCODE）" }

    Write-Host '验证 DPAPI 恢复和在线授权'
    cargo test --locked --lib account_store::tests:: -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) { throw "cargo test --locked --lib account_store::tests:: -- --ignored --nocapture 失败（退出码 $LASTEXITCODE）" }
} finally {
    $env:LOCALAPPDATA = $previousLocalAppData
    $env:GENSHIN_GO_TRACE_PASSPORT = $previousPassportTrace
    Pop-Location
}
