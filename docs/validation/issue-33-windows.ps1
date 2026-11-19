#requires -Version 7.2
param(
    [Parameter(Mandatory)]
    [ValidatePattern('\A[1-9][0-9]{8,9}\z')]
    [string]$UidA,

    [Parameter(Mandatory)]
    [ValidatePattern('\A[1-9][0-9]{8,9}\z')]
    [string]$UidB
)

$ErrorActionPreference = 'Stop'
if ($UidA -eq $UidB) { throw '请使用对应不同游戏 UID 的两个米哈游账号' }

$validationDirectory = Join-Path ([IO.Path]::GetTempPath()) ('genshin-go-renewal-' + [guid]::NewGuid())
$testDirectory = Join-Path $PSScriptRoot ('.issue-33-' + [guid]::NewGuid())
$testPath = Join-Path $testDirectory 'windows.test.ts'
$previousLocalAppData = $env:LOCALAPPDATA

function Invoke-LauncherValidation($Stage, $Steps) {
    Write-Host $Stage
    $script = @"
import { open, test } from "nsh/test";

test("Windows authorization", async () => {
const app = await open("../../../nsh.json", { network: "live" });
await app.wait(1200);
await app.setViewport(520, 900);
const uidA = "$UidA";
const uidB = "$UidB";
$Steps
}, 450000);
"@
    Set-Content -LiteralPath $testPath -Value $script -Encoding utf8NoBOM
    nsh test $testPath --target windows
    if ($LASTEXITCODE -ne 0) { throw "$Stage 失败（退出码 $LASTEXITCODE）" }
}

Push-Location (Join-Path $PSScriptRoot '../..')
try {
    $null = New-Item -ItemType Directory -Path $validationDirectory
    $null = New-Item -ItemType Directory -Path $testDirectory
    $env:LOCALAPPDATA = $validationDirectory
    Write-Host "验证目录 $validationDirectory（运行后保留）"
    Write-Host '第一阶段依次用 A、B、A 扫码。移除 A 后会再提示用 B 扫码。每次二维码出现后有两分钟。'

    Invoke-LauncherValidation '添加 A、误扫 B、续期 A' @'
const scan = async (button, account, result) => {
  console.log(`请使用账号 ${account} 扫码并确认`);
  await app.press(button);
  await app.waitFor("等待扫描：请使用米游社扫码", { timeout: 20000 });
  await app.waitFor(result, { timeout: 120000 });
};
await app.waitFor("尚未保存米哈游账号");
await scan("扫码添加米哈游账号", "A", "米游社授权已保存");
await app.waitFor(`已验证游戏 UID：${uidA}`);
const accountA = app.texts().find(text => text.startsWith("米哈游账号："));
if (!accountA) throw new Error("未显示账号 A");
const renew = accountA.replace("米哈游账号：", "重新扫码：");
await scan(renew, "B", "扫码账号与待更新账号不一致，请使用原账号重新扫码");
await app.waitFor(accountA);
await app.waitFor(`已验证游戏 UID：${uidA}`);
await scan(renew, "A", "米游社授权已续期");
await app.waitFor(accountA);
await app.waitFor(`已验证游戏 UID：${uidA}`);
'@

    Invoke-LauncherValidation '重开并移除已续期的账号 A' @'
await app.waitFor(`已验证游戏 UID：${uidA}`);
const accountA = app.texts().find(text => text.startsWith("米哈游账号："));
if (!accountA) throw new Error("重开后未恢复账号 A");
await app.press(accountA.replace("米哈游账号：", "移除账号："));
await app.waitFor("尚未保存米哈游账号");
await app.waitFor(`当前游戏 UID：${uidA}`);
'@

    Invoke-LauncherValidation '重开并添加 B' @'
await app.waitFor("尚未保存米哈游账号");
await app.waitFor(`当前游戏 UID：${uidA}`);
console.log("请使用账号 B 扫码并确认");
await app.press("扫码添加米哈游账号");
await app.waitFor("等待扫描：请使用米游社扫码", { timeout: 20000 });
await app.waitFor("米游社授权已保存", { timeout: 120000 });
await app.waitFor(`已验证游戏 UID：${uidB}`);
'@

    Invoke-LauncherValidation '重开后仅恢复 B，并检查两个 UID 数据空间' @'
await app.waitFor(`已验证游戏 UID：${uidB}`);
if (app.texts().filter(text => text.startsWith("米哈游账号：")).length !== 1)
  throw new Error("重开后应仅恢复一个米哈游账号");
await app.waitFor(`当前游戏 UID：${uidA}`);
await app.press(`切换至 UID：${uidB}`);
await app.waitFor(`当前游戏 UID：${uidB}`);
await app.press(`切换至 UID：${uidA}`);
await app.waitFor(`当前游戏 UID：${uidA}`);
await app.waitFor(`已验证游戏 UID：${uidB}`);
'@
    Write-Host '续期、移除、换账号和重开验证通过。'
} finally {
    $env:LOCALAPPDATA = $previousLocalAppData
    if (Test-Path -LiteralPath $testPath) { Remove-Item -LiteralPath $testPath }
    if (Test-Path -LiteralPath $testDirectory) { Remove-Item -LiteralPath $testDirectory }
    Pop-Location
}
