#requires -Version 7.2
param(
    [switch]$LiveScan,
    [switch]$KeepData
)

$ErrorActionPreference = 'Stop'
$tempRoot = [IO.Path]::TrimEndingDirectorySeparator((Get-Item -LiteralPath ([IO.Path]::GetTempPath())).FullName)
$validationDirectory = Join-Path $tempRoot ('genshin-go-upgrade-' + [guid]::NewGuid())
$previousDataDir = $env:GENSHIN_GO_DATA_DIR
$previousValidationLocalAppData = $env:GENSHIN_GO_VALIDATION_LOCALAPPDATA
Push-Location (Join-Path $PSScriptRoot '../..')
try {
    $null = New-Item -ItemType Directory -Path $validationDirectory
    Write-Host "验证目录 $validationDirectory"
    $env:GENSHIN_GO_VALIDATION_LOCALAPPDATA = $validationDirectory
    cargo test --locked --lib account_store::tests::prepare_windows_upgrade_validation -- --ignored --exact
    if ($LASTEXITCODE -ne 0) { throw "准备旧版数据失败（退出码 $LASTEXITCODE）" }

    $env:GENSHIN_GO_DATA_DIR = Join-Path $validationDirectory 'Genshin-Go'
    $oldAuthorization = Join-Path $validationDirectory 'Genshin-Go/accounts-passport.dpapi'
    @'
await app.setViewport(520, 900);
await app.press("米游社");
await app.waitFor("尚未保存米哈游账号");
await app.waitFor("扫码添加米哈游账号");
await app.press("角色资料");
await app.waitFor("当前游戏 UID：123456789");
await app.press("切换至 UID：987654321");
await app.waitFor("当前游戏 UID：987654321");
await app.press("米游社");
await app.waitFor("尚未保存米哈游账号");
'@ | nsh test nsh.json --target windows
    if ($LASTEXITCODE -ne 0) { throw "原生窗口升级验证失败（退出码 $LASTEXITCODE）" }
    if (Test-Path -LiteralPath $oldAuthorization) { throw '旧版授权文件仍存在' }

    if ($LiveScan) {
        Get-Content (Join-Path $PSScriptRoot 't02-windows-save.js') -Raw |
            nsh test nsh.json --target windows
        if ($LASTEXITCODE -ne 0) { throw "原生窗口扫码验证失败（退出码 $LASTEXITCODE）" }

        $restore = Get-Content (Join-Path $PSScriptRoot 't02-windows-restore.js') -Raw
        $restore += "`nawait app.press(`"角色资料`");`nawait app.waitFor(`"当前游戏 UID：987654321`");"
        foreach ($restart in 1..2) {
            $restore | nsh test nsh.json --target windows
            if ($LASTEXITCODE -ne 0) { throw "第 $restart 次重开恢复失败（退出码 $LASTEXITCODE）" }
        }
    }
} finally {
    $env:GENSHIN_GO_DATA_DIR = $previousDataDir
    $env:GENSHIN_GO_VALIDATION_LOCALAPPDATA = $previousValidationLocalAppData
    Pop-Location
}

if (-not $KeepData) {
    $createdDirectory = Get-Item -LiteralPath $validationDirectory -Force
    if (-not $createdDirectory.PSIsContainer -or
        $createdDirectory.FullName -ne $validationDirectory -or
        $createdDirectory.Parent.FullName -ne $tempRoot -or
        ($createdDirectory.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "无法确认验证目录位于临时目录下：$validationDirectory"
    }
    Remove-Item -LiteralPath $createdDirectory.FullName -Recurse -Force
    Write-Host "已清理验证目录 $validationDirectory"
}
