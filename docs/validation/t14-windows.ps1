#requires -Version 7.2
param([switch]$Native)

$ErrorActionPreference = 'Stop'
$testRoot = Join-Path $PSScriptRoot ('.t14-' + [guid]::NewGuid())
$testFile = Join-Path $testRoot 'wishes.test.ts'
$data = Join-Path ([IO.Path]::GetTempPath()) ('genshin-go-wishes-' + [guid]::NewGuid())
$previousDataDir = $env:GENSHIN_GO_DATA_DIR

function Write-Uigf($Path, $Version, $Groups, [switch]$WithOtherGame) {
    $archive = @{
        info = @{
            version = $Version
            export_timestamp = '1760000000'
            export_app = 'T14 validation'
            export_app_version = '1'
        }
        hk4e = $Groups
    }
    if ($WithOtherGame) { $archive.hkrpg = @() }
    $archive | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $Path -Encoding utf8
}

function Invoke-WishTest($Paths, $Steps) {
    $pathsJson = ConvertTo-Json -InputObject $Paths -Compress
    $source = @"
import { expect, open, test } from "nsh/test";

test("Windows wish records", async () => {
  const app = await open("../../../nsh.json", { width: 1024, height: 1000 });
  const paths: Record<string, string> = $pathsJson;
  await app.press("角色资料");
  await app.waitFor("祈愿记录");
  const create = async (uid: string) => {
    await app.type("game-uid", uid);
    await app.press("创建 UID 数据空间");
  };
  const read = async (path: string) => {
    await app.clear("uigf-file");
    await app.type("uigf-file", path);
    await app.press("读取 UIGF 文件");
  };
  const waitForPrefix = async (prefix: string) =>
    app.waitUntil(() => app.texts().some(line => line.startsWith(prefix)));
  $Steps
}, 90000);
"@
    Set-Content -LiteralPath $testFile -Value $source -Encoding utf8
    $targetArgument = if ($Native) { @('--target', 'windows') } else { @() }
    nsh test $testFile @targetArgument
    if ($LASTEXITCODE -ne 0) { throw "Windows 祈愿记录测试失败（退出码 $LASTEXITCODE）" }
}

Push-Location (Join-Path $PSScriptRoot '../..')
try {
    $null = New-Item -ItemType Directory -Path $data, $testRoot
    $appData = Join-Path $data 'Genshin-Go'
    $env:GENSHIN_GO_DATA_DIR = $appData
    Write-Host "本次验证的数据目录（运行后保留） $data"

    $paths = @{
        initial = Join-Path $data 'initial.json'
        merge = Join-Path $data 'merge.json'
        old = Join-Path $data 'old.json'
        invalid = Join-Path $data 'invalid.json'
        duplicateKey = Join-Path $data 'duplicate-key.json'
        broken = Join-Path $data 'broken.json'
        ambiguous = Join-Path $data 'ambiguous.json'
    }
    $first = @{
        id = '1000000000000000001'; item_id = '10000002'; name = '神里绫华'
        time = '2025-10-09 16:00:00'; rank_type = '5'; gacha_type = '400'; uigf_gacha_type = '301'
    }
    $conflict = $first.Clone()
    $conflict.item_id = '10000003'
    $second = @{
        id = '1000000000000000002'; item_id = '11401'; name = '西风剑'
        time = '2025-10-09 09:00:00'; rank_type = '4'; gacha_type = '302'; uigf_gacha_type = '302'
    }
    $duplicate = $first.Clone()
    $duplicate.time = '2025-10-09 08:00:00'
    $invalid = $first.Clone()
    $invalid.Remove('item_id')
    $withoutRank = $first.Clone()
    $withoutRank.Remove('rank_type')
    $otherRank = $first.Clone()
    $otherRank.rank_type = '4'
    Write-Uigf $paths.initial 'v4.0' @(@{ uid = 123456789; timezone = 8; list = @($withoutRank) })
    Write-Uigf $paths.ambiguous 'v4.2' @(@{ uid = 123456789; timezone = 8; list = @($first, $otherRank) })
    Write-Uigf $paths.merge 'v4.1' @(
        @{ uid = '123456789'; timezone = 0; list = @($duplicate, $conflict, $second) }
        @{ timezone = 8; list = @($first) }
    ) -WithOtherGame
    Write-Uigf $paths.invalid 'v4.2' @(@{ uid = '123456789'; timezone = 8; list = @($invalid) })
    '{"info":{"uigf_version":"v3.0"},"list":[]}' | Set-Content -LiteralPath $paths.old
    '{"info":' | Set-Content -LiteralPath $paths.broken
    (Get-Content -LiteralPath $paths.initial -Raw).Replace('"id":', '"id": "1", "id":') |
        Set-Content -LiteralPath $paths.duplicateKey

    # A file at the backup directory blocks the backup without touching the wish table.
    $null = New-Item -ItemType Directory -Path $appData
    $blockedBackup = Join-Path $appData 'wish-backups'
    $null = New-Item -ItemType File -Path $blockedBackup
    Invoke-WishTest $paths @'
  await read(paths.initial);
  await app.waitFor("已读取 1 条原神祈愿记录");
  await app.waitFor("请先创建 UID 123456789 的数据空间");
  await create("123456789");
  await app.press("预览导入至 UID 123456789");
  await app.waitFor("导入至 UID 123456789：新增 1，重复 0，冲突 0");
  await app.waitFor("暂无祈愿记录");
  await app.press("备份并确认导入");
  await waitForPrefix("导入失败：导入前备份失败");
  await app.waitFor("暂无祈愿记录");
'@
    Remove-Item -LiteralPath $blockedBackup

    Invoke-WishTest $paths @'
  await app.waitFor("当前游戏 UID：123456789");
  await read(paths.initial);
  await app.waitFor("导入至 UID 123456789：新增 1，重复 0，冲突 0");
  await app.waitFor("暂无祈愿记录");
  await app.press("备份并确认导入");
  await app.waitFor("2025-10-09 08:00:00 · 角色活动祈愿 · 神里绫华");
  await waitForPrefix("已导入 UID 123456789 的 1 条祈愿记录。导入前备份：");
  await read(paths.merge);
  await app.waitFor("已读取 4 条原神祈愿记录");
  await app.waitFor("未处理的数据：崩坏：星穹铁道");
  expect(app.texts()).not.toContain("备份并确认导入");
  await app.press("预览导入至 UID 123456789");
  await app.waitFor("导入至 UID 123456789：新增 1，重复 1，冲突 1");
  await app.press("备份并确认导入");
  await app.waitFor("2025-10-09 09:00:00 · 武器活动祈愿 · 西风剑 · 4 星");
  await app.press("导出 UIGF v4.2");
  await waitForPrefix("UID 123456789 的 UIGF v4.2 已导出：");

  await create("987654321");
  await app.press("记录组 2（UID 无法识别）");
  await app.waitFor("文件 UID 无法识别，请明确选择导入目标");
  expect(app.texts()).not.toContain("备份并确认导入");
  await app.press("预览导入至 UID 987654321");
  await app.waitFor("导入至 UID 987654321：新增 1，重复 0，冲突 0");
  await app.press("备份并确认导入");
  await app.waitFor("2025-10-09 08:00:00 · 角色活动祈愿 · 神里绫华 · 5 星");
  expect(app.texts()).not.toContain("2025-10-09 09:00:00 · 武器活动祈愿 · 西风剑 · 4 星");
'@

    $exports = @(Get-ChildItem -LiteralPath (Join-Path $appData 'wish-exports') -Filter 'UIGF-123456789-*.json' -File)
    if ($exports.Count -ne 1) { throw '应为首个 UID 生成一个导出文件' }
    $paths.exported = $exports[0].FullName
    $export = Get-Content -LiteralPath $paths.exported -Raw | ConvertFrom-Json
    if ($export.info.version -ne 'v4.2' -or $export.hk4e[0].uid -ne '123456789' -or
        $export.hk4e[0].timezone -ne 0 -or $export.hk4e[0].list.Count -ne 2 -or
        $export.hk4e[0].list[1].item_id -ne $first.item_id -or
        $export.hk4e[0].list[1].id -cne $first.id) {
        throw '导出版本、UID、时间或记录错误'
    }
    $backupCounts = @(Get-ChildItem -LiteralPath $blockedBackup -Filter 'UIGF-123456789-*.json' -File |
        ForEach-Object {
            $backup = Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json
            if ($backup.PSObject.Properties.Name.Count -ne 2 -or $backup.hk4e[0].uid -ne '123456789') {
                throw '备份应仅包含所选 UID 的 UIGF 祈愿记录'
            }
            if ($backup.hk4e[0].list.Count -eq 1 -and $backup.hk4e[0].list[0].id -cne $first.id) {
                throw '第二次导入备份应包含第一次导入的记录'
            }
            $backup.hk4e[0].list.Count
        } | Sort-Object)
    if (($backupCounts -join ',') -ne '0,1') { throw '两次备份应分别包含零条和一条导入前记录' }

    Invoke-WishTest $paths @'
  await app.waitFor("当前游戏 UID：987654321");
  await app.waitFor("共 1 条 · 第 1 页 · 时间 UTC");
  await app.press("切换至 UID：123456789");
  await app.waitFor("共 2 条 · 第 1 页 · 时间 UTC");
  await read(paths.exported);
  await app.press("预览导入至 UID 123456789");
  await app.waitFor("导入至 UID 123456789：新增 0，重复 2，冲突 0");
  expect(app.texts()).not.toContain("备份并确认导入");

  await read(paths.old);
  await app.waitFor("不支持 UIGF v3.0，请选择 v4.0–v4.2 文件");
  await read(paths.invalid);
  await waitForPrefix("UIGF 字段无效：missing field");
  await read(paths.duplicateKey);
  await waitForPrefix("UIGF 字段无效：duplicate field");
  await read(paths.broken);
  await waitForPrefix("UIGF JSON 损坏：");
  await read(paths.ambiguous);
  await app.press("预览导入至 UID 123456789");
  await app.waitFor("导入至 UID 123456789：新增 0，重复 0，冲突 2");
  await app.waitFor("共 2 条 · 第 1 页 · 时间 UTC");

  await app.press("资料管理");
  app.onDialog(() => "确认删除 UID：123456789");
  await app.press("删除历史记录");
  await app.waitFor("暂无祈愿记录");
  await read(paths.exported);
  await app.press("预览导入至 UID 123456789");
  await app.waitFor("导入至 UID 123456789：新增 2，重复 0，冲突 0");
  await app.press("备份并确认导入");
  await app.waitFor("共 2 条 · 第 1 页 · 时间 UTC");
  await app.press("切换至 UID：987654321");
  await app.waitFor("共 1 条 · 第 1 页 · 时间 UTC");
'@
} finally {
    $env:GENSHIN_GO_DATA_DIR = $previousDataDir
    Remove-Item -LiteralPath $testFile -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $testRoot -ErrorAction SilentlyContinue
    Pop-Location
}
