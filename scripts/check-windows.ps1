#requires -Version 7.2
$ErrorActionPreference = 'Stop'

Push-Location (Join-Path $PSScriptRoot '..')
try {
    Write-Host 'Rust 格式检查'
    cargo fmt --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt --check 失败（退出码 $LASTEXITCODE）" }
    Write-Host 'Rust Clippy 检查'
    cargo clippy --locked --workspace --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy --locked --workspace --all-targets -- -D warnings 失败（退出码 $LASTEXITCODE）" }
    Write-Host 'Issue 同步脚本离线测试'
    ./docs/specs/test-launcher-ticket-sync.ps1

    Write-Host 'NSH 根项目检查及测试'
    nsh check
    if ($LASTEXITCODE -ne 0) { throw "nsh check 失败（退出码 $LASTEXITCODE）" }
    nsh test
    if ($LASTEXITCODE -ne 0) { throw "nsh test 失败（退出码 $LASTEXITCODE）" }

    Push-Location tests/.fixture
    try {
        Write-Host 'NSH 隔离夹具检查及测试'
        nsh check
        if ($LASTEXITCODE -ne 0) { throw "nsh check 失败（退出码 $LASTEXITCODE）" }
        nsh test
        if ($LASTEXITCODE -ne 0) { throw "nsh test 失败（退出码 $LASTEXITCODE）" }
    } finally {
        Pop-Location
    }
} finally {
    Pop-Location
}
