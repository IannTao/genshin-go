// The Rust root tested from TypeScript: `nsh test` runs this file in a
// headless runner it builds with the crate, and `open(".")` starts the root
// (`nsh.json` "root").
import { expect, open, test } from "nsh/test";
import { platform } from "process";

const windowsTest = platform === "windows" ? test : test.skip;

test("opens the launcher home and reaches installation controls", async () => {
  const app = await open(".");
  expect(app.texts()).toContain("原神启动器");
  await app.press("管理游戏");
  expect(app.texts()).not.toContain("选择游戏目录");
  await app.press("更换目录");
  expect(app.texts()).toContain("选择游戏目录");
  expect(app.texts()).not.toContain("普通启动");
});

windowsTest("opens a separate achievement tab from the launcher sidebar", async () => {
  const app = await open(".");
  await app.press("成就");
  expect(app.texts()).toContain("成就进度");
  expect(app.texts()).not.toContain("祈愿记录");
});

windowsTest("previews and imports UIAF into an explicitly selected UID", async () => {
  const app = await open(".");
  await app.press("角色资料");
  await app.type("game-uid", "123450018");
  await app.press("创建 UID 数据空间");
  await app.type("game-uid", "123450019");
  await app.press("创建 UID 数据空间");
  await app.press("成就");
  await app.type("uiaf-file", "tests/data/uiaf-v1.1.json");
  await app.press("读取 UIAF 文件");
  expect(app.texts()).toContain("文件未声明 UID，请选择导入目标");
  expect(app.texts()).not.toContain("备份并确认导入");
  await app.press("预览导入至 UID 123450018");
  expect(app.texts()).toContain("导入至 UID 123450018：新增 2，更新 0，无变化 0");
  expect(app.texts()).toContain("暂无成就进度");
  await app.press("备份并确认导入");
  expect(app.texts().join("\n")).toContain("已导入 UID 123450018 的成就进度。导入前备份：");
  expect(app.texts()).toContain("暂无成就进度");
  await app.press("角色资料");
  await app.press("切换至 UID：123450018");
  await app.press("成就");
  expect(app.texts()).toContain("成就 80001 · 已领取 · 进度 1");
  expect(app.texts()).toContain("成就 80002 · 未完成 · 进度 30");
  await app.press("预览导入至 UID 123450018");
  expect(app.texts()).toContain("导入至 UID 123450018：新增 0，更新 0，无变化 2");
  expect(app.texts()).toContain("重复 2，冲突 0（冲突按状态与进度自动合并）");
  expect(app.texts()).not.toContain("备份并确认导入");
  await app.press("导出 UIAF v1.1");
  expect(app.texts().join("\n")).toContain("UID 123450018 的 UIAF v1.1 已导出：");
});
