#requires -Version 7.2
$ErrorActionPreference = 'Stop'
$validationDirectory = Join-Path ([IO.Path]::GetTempPath()) ('genshin-go-t05-' + [guid]::NewGuid())
$previousLocalAppData = $env:LOCALAPPDATA
Push-Location (Join-Path $PSScriptRoot '../..')
try {
    $env:LOCALAPPDATA = $validationDirectory
    Write-Host "验证目录 $validationDirectory"
    Write-Host '请使用同一个米游社账号确认两次扫码。每次二维码出现后有两分钟。'
    @'
await app.setViewport(520, 900);
const scan = async () => {
  await app.press("米游社");
  await app.press("扫码添加米哈游账号");
  await app.waitFor("等待扫描：请使用米游社扫码", { timeout: 20000 });
  await app.waitFor("米游社授权已保存", { timeout: 120000 });
};
await scan();
const account = app.texts().find(text => text.startsWith("米哈游账号："));
const verified = app.texts().find(text => text.startsWith("已验证游戏 UID："));
if (!account || !verified) throw new Error("未找到已验证的米哈游账号及游戏 UID");
const uid = verified.slice("已验证游戏 UID：".length);
await app.press("管理授权");
app.onDialog(() => account.replace("米哈游账号：", "确认移除账号："));
await app.press(account.replace("米哈游账号：", "移除账号："));
await app.waitFor("尚未保存米哈游账号");
await app.press("角色资料");
await app.waitFor("当前游戏 UID：" + uid);
await scan();
await app.waitFor(account);
await app.waitFor(verified);
await app.press("角色资料");
await app.waitFor("当前游戏 UID：" + uid);
if (app.texts().some(text => text.startsWith("切换至 UID：")))
  throw new Error("复扫后不应新增其他 UID 数据空间");
await app.press("资料管理");
app.onDialog(() => "取消");
await app.press("删除历史记录");
if (app.dialogs().at(-1)?.title !== `删除 UID ${uid} 的历史记录？`)
  throw new Error("确认框应显示原 UID");
'@ | nsh test nsh.json --target windows
    if ($LASTEXITCODE -ne 0) { throw "原生扫码复验失败（退出码 $LASTEXITCODE）" }

    Get-Content (Join-Path $PSScriptRoot 't02-windows-restore.js') -Raw |
        nsh test nsh.json --target windows
    if ($LASTEXITCODE -ne 0) { throw "重开原生窗口复验失败（退出码 $LASTEXITCODE）" }
} finally {
    $env:LOCALAPPDATA = $previousLocalAppData
    Pop-Location
}
