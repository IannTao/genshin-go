#requires -Version 7.2
$ErrorActionPreference = 'Stop'

$tempRoot = [IO.Path]::TrimEndingDirectorySeparator((Get-Item -LiteralPath ([IO.Path]::GetTempPath())).FullName)
$testData = Join-Path $tempRoot ('genshin-go-check-' + [guid]::NewGuid())
$previousDataDir = $env:GENSHIN_GO_DATA_DIR

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
    $createdDirectory = New-Item -ItemType Directory -Path $testData
    try {
        $env:GENSHIN_GO_DATA_DIR = $createdDirectory.FullName
        nsh test
        if ($LASTEXITCODE -ne 0) { throw "nsh test 失败（退出码 $LASTEXITCODE）" }
        if (-not (Test-Path -LiteralPath (Join-Path $testData 'launcher.sqlite3') -PathType Leaf)) {
            throw 'NSH 根组件测试未在临时应用数据目录创建数据库'
        }
    } finally {
        $env:GENSHIN_GO_DATA_DIR = $previousDataDir
        $resolvedTestData = Get-Item -LiteralPath $createdDirectory.FullName -Force
        if (-not $resolvedTestData.PSIsContainer -or
            $resolvedTestData.FullName -ne $testData -or
            $resolvedTestData.Parent.FullName -ne $tempRoot -or
            ($resolvedTestData.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "无法确认测试数据位于临时目录下：$testData"
        }
        Remove-Item -LiteralPath $resolvedTestData.FullName -Recurse -Force
    }

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
