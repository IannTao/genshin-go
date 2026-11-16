import { expect, open, test } from "nsh/test";

test("starts QR authorization and can cancel without adding an account", async () => {
  const app = await open(".");
  await app.press("扫码添加米哈游账号");
  expect(app.texts()).toContain("等待扫描：请使用米游社扫码");
  await app.press("取消扫码");
  expect(app.texts()).toContain("已取消扫码");
  expect(app.texts()).toContain("尚未保存米哈游账号");
});

test("confirms authorization, saves accounts and restores them after reopening", async () => {
  const app = await open(".");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("授权已保存；尚未登录游戏"));
  expect(app.texts()).toContain("米哈游账号：10001");
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("米哈游账号：10001");
});

test("shows scanned and expired states and refreshes an expired QR", async () => {
  const app = await open(".");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟已扫描");
  await app.waitUntil(() => app.texts().includes("等待确认：请在米游社确认授权"));
  await app.press("模拟二维码过期");
  await app.waitUntil(() => app.texts().includes("二维码已过期，请刷新二维码"));
  await app.press("刷新二维码");
  expect(app.texts()).toContain("等待扫描：请使用米游社扫码");
});

test("offers reauthorization for an expired account without duplicating it", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("授权已保存；尚未登录游戏"));
  await app.press("检查授权：10001");
  expect(app.texts()).toContain("账号 10001 授权已失效，请重新扫码");
  await app.press("重新扫码：10001");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("授权已保存；尚未登录游戏"));
  await app.press("重新打开启动器");
  expect(app.texts().filter(t => t === "米哈游账号：10001").length).toBe(1);
});

test("reports phone cancellation and network failure without saving authorization", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟手机取消");
  await app.waitUntil(() => app.texts().includes("已取消扫码"));
  await app.press("刷新二维码");
  await app.press("模拟扫码网络失败");
  await app.waitUntil(() => app.texts().includes("网络失败：无法连接米哈游服务，请重试"));
  expect(app.texts()).toContain("尚未保存米哈游账号");
  await app.press("刷新二维码");
  expect(app.texts()).toContain("等待扫描：请使用米游社扫码");
});

test("does not save a confirmed authorization after cancellation", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.press("取消扫码");
  await app.wait(4000);
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("尚未保存米哈游账号");
});

test("reports service risk rejection without saving authorization", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟服务风控");
  await app.waitUntil(() => app.texts().includes("扫码服务拒绝请求：设备或网络环境存在风险（-3503）"));
  expect(app.texts()).toContain("尚未保存米哈游账号");
});

test("restores two distinct MiHoYo accounts after reopening", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("米哈游账号：10001"));
  await app.press("模拟另一米哈游账号");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("米哈游账号：10002"));
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("米哈游账号：10001");
  expect(app.texts()).toContain("米哈游账号：10002");
});

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
