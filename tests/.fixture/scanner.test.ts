import { expect, open, test } from "nsh/test";
import { platform } from "process";

const windowsTest = platform === "windows" ? test : test.skip;

async function configuredScanner() {
  const app = await open(".", { width: 1100, height: 1000 });
  await app.press("模拟扫描服务");
  const setting = (prefix: string) => {
    const line = app.texts().find(value => value.startsWith(prefix));
    if (!line) throw new Error(prefix);
    return line.slice(prefix.length);
  };
  const program = setting("模拟程序：");
  const port = setting("模拟端口：");
  await app.press("游戏管理");
  await app.type("scanner-program", program);
  await app.clear("scanner-port");
  await app.type("scanner-port", port);
  await app.press("保存扫描设置");
  await app.waitFor("已保存扫描设置");
  return { app, setting };
}

windowsTest("routes a new login to its own persisted roster from ordinary launch", async () => {
  const { app, setting } = await configuredScanner();
  await app.press("检查扫描连接");
  await app.waitFor("已连接原神持续同步服务");
  await app.press("角色资料");
  await app.type("game-uid", "123450042");
  await app.press("创建 UID 数据空间");
  await app.press("重新打开启动器");
  await app.press("选择：D:/原神");
  await app.press("确认保存目录");
  await app.press("普通启动");
  await app.waitFor("已请求启动游戏");
  await app.waitFor("等待本次登录的角色数据");
  await app.press("普通启动");
  expect(app.texts()).toContain("游戏启动次数：1");
  await app.press("模拟旧会话更新");
  await app.wait(2300);
  expect(app.texts()).toContain("等待本次登录的角色数据");
  await app.press("模拟角色尚未到达");
  await app.waitFor("等待本次登录的完整角色列表");
  await app.press("模拟过期角色修订");
  await app.wait(2300);
  expect(app.texts()).toContain("等待本次登录的完整角色列表");
  await app.press("准备新登录快照");
  await app.wait(150);
  const published = Date.now();
  await app.press("发布新登录快照");
  await app.waitFor("已同步 UID 987650042 的 2 条已拥有角色记录");
  const completed = Date.now();
  await app.press("重新打开启动器");
  await app.press("角色资料");
  expect(app.texts()).toContain("当前游戏 UID：123450042");
  expect(app.texts()).toContain("已拥有角色尚未同步");
  await app.press("切换至 UID：987650042");
  expect(app.texts()).toContain("Traveler（Anemo） · 等级 80 · 突破 5 · 命座 4 · 天赋 6/7/8");
  expect(app.texts()).toContain("Traveler（Geo） · 等级 80 · 突破 5 · 命座 2 · 天赋 5/6/7");
  const source = new Date(Number(setting("模拟来源毫秒："))).toISOString().replace("T", " ").replace("Z", " UTC");
  expect(app.texts()).toContain("来源时间：" + source);
  const received = setting("本机接收时间：");
  const receivedMs = Date.parse(received.replace(" ", "T").replace(" UTC", "Z"));
  expect(receivedMs).toBeGreaterThan(Number(setting("模拟来源毫秒：")));
  expect(receivedMs).toBeGreaterThanOrEqual(published);
  expect(receivedMs).toBeLessThanOrEqual(completed);
  expect(app.texts()).not.toContain("切换至 UID：111110042");
  await app.press("模拟等待登录");
  await app.press("首页");
  await app.press("普通启动");
  await app.waitFor("等待本次登录的角色数据");
  await app.press("模拟无效角色列表");
  await app.waitFor("扫描失败：GGScanner 的角色列表无效");
  await app.press("角色资料");
  expect(app.texts()).toContain("角色记录：2 条");
  expect(app.texts()).toContain("本机接收时间：" + received);
}, 60000);

windowsTest("launches normally when continuous sync is disabled or capture fails", async () => {
  const { app } = await configuredScanner();
  await app.press("模拟等待登录");
  await app.press("检查扫描连接");
  await app.waitFor("已连接持续同步服务，等待游戏登录");
  await app.press("首页");
  await app.press("选择：D:/原神");
  await app.press("确认保存目录");
  for (const [scenario, failure] of [
    ["模拟服务禁用", "持续同步已禁用，请在 GGScanner 勾选持续同步并重启原神管理服务"],
    ["模拟采集失败", "GGScanner 采集失败，请查看扫描器诊断并重启管理服务"],
    ["模拟来源缺失", "GGScanner 快照格式无效"],
    ["模拟 UID 无效", "GGScanner 快照缺少可信游戏 UID"],
  ]) {
    await app.press(scenario);
    await app.press("普通启动");
    await app.waitFor("扫描失败：" + failure);
    expect(app.texts()).toContain("已请求启动游戏");
  }
  await app.press("模拟安装已移除");
  await app.press("普通启动");
  expect(app.texts().join("\n")).toContain("启动失败：目录已失效");
  expect(app.texts()).toContain("游戏启动次数：4");
}, 60000);
