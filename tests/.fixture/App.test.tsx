import { expect, open, test } from "nsh/test";

test("chooses a discovered installation and keeps the default after reopening", async () => {
  const app = await open(".");
  await app.press("选择：D:/原神");
  expect(app.texts()).toContain("默认目录：D:/原神");
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("默认目录：D:/原神");
  await app.press("普通启动");
  expect(app.texts()).toContain("已请求启动游戏");
  expect(app.texts()).toContain("请求启动：D:/原神");
});

test("chooses a directory from the picker and can also enter a valid path", async () => {
  const app = await open(".");
  await app.press("选择游戏目录");
  expect(app.texts()).toContain("默认目录：E:/手动目录");
  await app.type("game-directory", "D:/原神");
  await app.press("使用此目录");
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("默认目录：D:/原神");
});

test("revalidates the default before launch when an installation disappears", async () => {
  const app = await open(".");
  await app.press("选择：D:/原神");
  await app.press("模拟安装已移除");
  await app.press("普通启动");
  expect(app.texts()).toContain("启动失败：目录已失效：不是国服官服游戏目录。请重新选择游戏目录。");
  expect(app.texts()).not.toContain("请求启动：D:/原神");
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("默认目录已失效：不是国服官服游戏目录。请重新选择游戏目录。");
  expect(app.texts()).toContain("尚未选择默认目录");
});

test("does not claim to save a new default when settings cannot be written", async () => {
  const app = await open(".");
  await app.press("选择：C:/原神");
  await app.press("模拟配置写入失败");
  await app.press("选择：D:/原神");
  expect(app.texts()).toContain("无法选择目录：配置不可写");
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("默认目录：C:/原神");
});

test("reports a system launch failure without reporting success", async () => {
  const app = await open(".");
  await app.press("选择：D:/原神");
  await app.press("模拟系统启动失败");
  await app.press("普通启动");
  expect(app.texts()).toContain("启动失败：系统拒绝启动");
  expect(app.texts()).not.toContain("已请求启动游戏");
});

test("gives installation guidance and blocks launch when no game exists", async () => {
  const app = await open(".");
  await app.press("模拟安装已移除");
  await app.press("重新查找");
  expect(app.texts()).toContain("未找到游戏，请先安装国服官服，再选择游戏目录。");
  await app.press("普通启动");
  expect(app.texts()).toContain("启动失败：请先选择有效游戏目录");
});

test("rejects an invalid manual directory without replacing the saved default", async () => {
  const app = await open(".");
  await app.press("选择：C:/原神");
  await app.type("game-directory", "E:/not-a-game");
  await app.press("使用此目录");
  expect(app.texts()).toContain("无法选择目录：不是国服官服游戏目录");
  expect(app.texts()).toContain("默认目录：C:/原神");
});
