import { expect, open, test } from "nsh/test";

test("shows the actual reward for today's community sign-in in the calendar", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitFor("社区签到奖励日历");
  await app.waitFor("1970-01-02 · 待处理");
  await app.waitFor("1970-01-02 · 已领取 · 30 米游币");
  expect(app.texts()).not.toContain("重试社区签到");
});

test("automatically checks community sign-in for the saved account when reopening", async () => {
  const app = await open(".", { clock: {} });
  expect(app.texts()).toContain("签到请求次数：0");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitFor("米游社授权已保存");
  await app.press("重新打开启动器");
  await app.waitFor("社区签到：待处理");
  await app.waitFor("社区签到：已签到");
  expect(app.texts()).toContain("签到账号：10001");
  expect(app.texts()).not.toContain("重试社区签到");
});

test("checks community sign-in again after a new day and a same-day resume", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitFor("社区签到：已签到");
  await app.press("模拟休眠恢复");
  await app.waitFor("签到请求次数：2");
  await app.waitFor("社区签到：已签到");
  await app.type("game-uid", "987654321");
  await app.press("创建 UID 数据空间");
  await app.press("模拟签到网络失败");
  await app.press("模拟签到跨日");
  await app.waitFor("社区签到：失败，无法连接米游社");
  expect(app.texts()).toContain("1970-01-03 · 失败");
  expect(app.texts()).toContain("1970-01-02 · 已领取 · 30 米游币");
  expect(app.texts()).not.toContain("1970-01-03 · 已领取 · 30 米游币");
  await app.press("模拟当天签到尚未完成");
  await app.press("模拟休眠恢复");
  await app.waitFor("社区签到：成功");
  expect(app.texts()).toContain("签到账号：10001");
  expect(app.texts()).toContain("当前游戏 UID：987654321");
  await app.press("模拟签到网络失败");
  await app.press("模拟休眠恢复");
  await app.waitFor("签到请求次数：5");
  await app.wait(2000);
  expect(app.texts()).toContain("社区签到：成功");
  expect(app.texts()).not.toContain("重试社区签到");
});

test("bounds community sign-in timeout retries and lets the user retry unfinished sign-in", async () => {
  const app = await open(".", { clock: {} });
  await app.press("模拟签到超时");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitFor("社区签到：待处理");
  expect(app.texts()).toContain("1970-01-02 · 待处理");
  await app.wait(7000);
  expect(app.texts()).toContain("社区签到：失败，网络超时，请重试");
  expect(app.texts()).toContain("1970-01-02 · 失败");
  expect(app.texts()).toContain("签到请求次数：3");
  await app.press("模拟当天签到尚未完成");
  await app.press("重试社区签到");
  await app.waitFor("社区签到：成功");
  expect(app.texts()).toContain("1970-01-02 · 已领取 · 30 米游币");
  expect(app.texts()).toContain("签到请求次数：4");
  expect(app.texts()).not.toContain("重试社区签到");
});

test("keeps the reward unconfirmed when only community sign-in is confirmed", async () => {
  const app = await open(".", { clock: {} });
  await app.press("模拟签到已完成但奖励未确认");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitFor("1970-01-02 · 已签到 · 领取待确认");
  expect(app.texts()).not.toContain("1970-01-02 · 已领取 · 30 米游币");
  expect(app.texts()).not.toContain("重试社区签到");
  await app.press("模拟当天签到尚未完成");
  await app.press("检查奖励领取");
  await app.waitFor("签到请求次数：2");
  await app.wait(2000);
  expect(app.texts()).toContain("1970-01-02 · 已签到 · 领取待确认");
  await app.press("模拟签到状态已确认领取");
  await app.press("检查奖励领取");
  await app.waitFor("1970-01-02 · 已领取 · 30 米游币");
  expect(app.texts()).not.toContain("检查奖励领取");
});

test("does not carry a reward check into the next day's blocked sign-in", async () => {
  const app = await open(".", { clock: {} });
  await app.press("模拟签到已完成但奖励未确认");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitFor("1970-01-02 · 已签到 · 领取待确认");
  await app.press("模拟签到需要人工验证");
  await app.press("检查奖励领取");
  await app.waitFor("社区签到：失败，请在米游社完成验证或签到后重试");

  await app.press("模拟当天签到尚未完成");
  await app.press("检查奖励领取");
  await app.press("模拟签到跨日");
  await app.waitFor("1970-01-03 · 待处理");
  await app.wait(4000);
  expect(app.texts()).toContain("1970-01-02 · 已签到 · 领取待确认");
  expect(app.texts()).toContain("签到请求次数：2");
});

test("waits for the user when community sign-in requires verification or a new scan", async () => {
  const app = await open(".", { clock: {} });
  await app.press("模拟签到需要人工验证");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitFor("签到请求次数：1");
  await app.press("模拟签到跨日");
  await app.waitFor("社区签到：失败，请在米游社完成验证或签到后重试");
  await app.press("模拟休眠恢复");
  await app.press("模拟签到跨日");
  await app.wait(4000);
  expect(app.texts()).toContain("签到请求次数：1");

  await app.press("模拟签到授权失效");
  await app.press("重试社区签到");
  await app.waitFor("社区签到：失败，签到授权不可用，请重新扫码");
  expect(app.texts()).not.toContain("重试社区签到");
  await app.press("模拟签到跨日");
  await app.wait(4000);
  expect(app.texts()).toContain("签到请求次数：2");

  await app.press("模拟当天签到尚未完成");
  await app.press("重新扫码：10001");
  await app.press("模拟扫码确认新凭据");
  await app.waitFor("社区签到：成功");
  expect(app.texts()).toContain("签到请求次数：3");
});

test("cancels pending community sign-in retries when the saved account is removed", async () => {
  const app = await open(".", { clock: {} });
  await app.press("模拟签到超时");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitFor("签到请求次数：1");
  await app.press("移除账号：10001");
  await app.wait(7000);
  expect(app.texts()).toContain("签到请求次数：1");
  expect(app.texts()).not.toContain("社区签到：待处理");
});

test("pauses a new installation and waits for manual resume after reopening", async () => {
  const app = await open(".", { clock: {} });
  const texts = () => app.texts().map(text => text.replaceAll("\\", "/"));
  await app.press("选择安装位置");
  expect(texts()).toContain("安装目录：E:/手动目录/Genshin Impact Game");
  await app.press("开始下载");
  await app.waitFor("已写入 1%（1 / 100 字节）");
  await app.press("暂停下载");
  await app.waitFor("下载已暂停");
  await app.press("继续下载");
  await app.waitFor("正在下载并安装…");
  await app.press("重新打开启动器");
  await app.waitFor("有未完成的安装，等待手动继续");
  await app.wait(5000);
  expect(app.texts()).toContain("继续下载");
  expect(texts()).not.toContain("默认目录：E:/手动目录/Genshin Impact Game");

  await app.press("继续下载");
  await app.waitFor("游戏下载并安装完成");
  expect(texts()).toContain("默认目录：E:/手动目录/Genshin Impact Game");
  await app.press("重新打开启动器");
  expect(texts()).toContain("默认目录：E:/手动目录/Genshin Impact Game");
  expect(texts()).not.toContain("继续下载");
  await app.press("普通启动");
  expect(texts()).toContain("请求启动：E:/手动目录/Genshin Impact Game");
});

test("reports a competing first save and displays the account that won", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟另一启动器先保存授权");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("保存米游社授权失败：本机授权已在另一启动器中更改"));
  expect(app.texts()).toContain("米哈游账号：10002");
  expect(app.texts()).toContain("已验证游戏 UID：987654321");
  expect(app.texts()).toContain("重新扫码：10002");
  expect(app.texts()).not.toContain("米游社授权已保存");
  expect(app.texts()).not.toContain("扫码添加米哈游账号");
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("米哈游账号：10002");
});

test("creates and switches independent game UID spaces without signing in", async () => {
  const app = await open(".");
  await app.type("game-uid", "123456789");
  await app.press("创建 UID 数据空间");
  await app.type("game-uid", "987654321");
  await app.press("创建 UID 数据空间");
  expect(app.texts()).toContain("当前游戏 UID：987654321");
  await app.press("切换至 UID：123456789");
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("当前游戏 UID：123456789");
  expect(app.texts()).toContain("切换至 UID：987654321");
  expect(app.texts()).toContain("尚未保存米哈游账号");
});

test("starts QR authorization and can cancel without adding an account", async () => {
  const app = await open(".");
  await app.press("扫码添加米哈游账号");
  expect(app.texts()).toContain("等待扫描：请使用米游社扫码");
  await app.press("取消扫码");
  expect(app.texts()).toContain("已取消扫码");
  expect(app.texts()).toContain("尚未保存米哈游账号");
});

test("links only the verified UID without changing the selected manual space", async () => {
  const app = await open(".");
  await app.type("game-uid", "987654321");
  await app.press("创建 UID 数据空间");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("米游社授权已保存"));
  expect(app.texts()).toContain("米哈游账号：10001");
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("米哈游账号：10001");
  expect(app.texts()).toContain("已验证游戏 UID：123456789");
  expect(app.texts()).toContain("当前游戏 UID：987654321");
  expect(app.texts()).toContain("切换至 UID：123456789");
});

test("retries a failed first scan and restores only the verified account's controls", async () => {
  const app = await open(".", { clock: {} });
  await app.type("game-uid", "123456789");
  await app.press("创建 UID 数据空间");
  await app.press("模拟查不到游戏 UID");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("未查到该米哈游账号的游戏 UID"));
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("尚未保存米哈游账号");
  expect(app.texts()).toContain("当前游戏 UID：123456789");

  await app.press("模拟查到唯一游戏 UID");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("米游社授权已保存"));
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("米哈游账号：10001");
  expect(app.texts()).toContain("已验证游戏 UID：123456789");
  expect(app.texts()).toContain("检查授权：10001");
  expect(app.texts()).toContain("重新扫码：10001");
  expect(app.texts()).toContain("移除账号：10001");
  expect(app.texts()).not.toContain("扫码添加米哈游账号");
  expect(app.texts()).not.toContain("刷新二维码");
  expect(app.texts()).not.toContain("取消扫码");
});

test("removes the old authorization and restores only the new account with both UID spaces", async () => {
  const app = await open(".", { clock: {} });
  await app.type("game-uid", "987654321");
  await app.press("创建 UID 数据空间");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("米游社授权已保存"));

  await app.press("重新扫码：10001");
  await app.press("模拟扫码确认");
  await app.press("移除账号：10001");
  await app.wait(4000);
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("尚未保存米哈游账号");
  expect(app.texts()).toContain("当前游戏 UID：987654321");
  expect(app.texts()).toContain("切换至 UID：123456789");

  await app.press("模拟另一米哈游账号");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("米游社授权已保存"));
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("米哈游账号：10002");
  expect(app.texts()).not.toContain("米哈游账号：10001");
  expect(app.texts()).toContain("已验证游戏 UID：987654321");
  expect(app.texts()).toContain("当前游戏 UID：987654321");
  await app.press("切换至 UID：123456789");
  expect(app.texts()).toContain("当前游戏 UID：123456789");
  await app.press("切换至 UID：987654321");
  expect(app.texts()).toContain("当前游戏 UID：987654321");
  expect(app.texts()).toContain("已验证游戏 UID：987654321");
});

test("shows the UID scope before confirming history deletion", async () => {
  const app = await open(".", { clock: {} });
  await app.type("game-uid", "987654321");
  await app.press("创建 UID 数据空间");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("米游社授权已保存"));

  await app.press("删除历史记录");
  expect(app.texts()).toContain("选择 UID：123456789");
  expect(app.texts()).toContain("选择 UID：987654321");
  await app.press("选择 UID：987654321");
  expect(app.texts()).toContain("本次删除涉及的 UID：987654321");
  expect(app.texts()).not.toContain("本次删除涉及的 UID：123456789");
  await app.press("取消删除历史记录");
  expect(app.texts()).not.toContain("已清除 UID 987654321 的本机历史记录");
  expect(app.texts()).not.toContain("确认删除 UID：987654321");

  await app.press("选择 UID：123456789");
  expect(app.texts()).toContain("本次删除涉及的 UID：123456789");
  await app.press("确认删除 UID：123456789");
  expect(app.texts()).toContain("已清除 UID 123456789 的本机历史记录");
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("已验证游戏 UID：123456789");
  expect(app.texts()).toContain("切换至 UID：123456789");
  expect(app.texts()).toContain("当前游戏 UID：987654321");
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

test("commits renewed credentials only after a successful write", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("米游社授权已保存"));
  await app.press("检查授权：10001");
  expect(app.texts()).toContain("账号 10001 授权已失效，请重新扫码");

  await app.press("模拟下次授权写入失败");
  await app.press("重新扫码：10001");
  await app.press("模拟扫码确认新凭据");
  await app.waitFor("保存米游社授权失败：授权数据库不可写");
  expect(app.texts()).toContain("米哈游账号：10001");
  expect(app.texts()).toContain("已验证游戏 UID：123456789");
  await app.press("重新打开启动器");
  await app.press("检查授权：10001");
  expect(app.texts()).toContain("账号 10001 授权已失效，请重新扫码");

  await app.press("重新扫码：10001");
  await app.press("模拟扫码确认新凭据");
  await app.waitFor("米游社授权已续期");
  await app.press("重新打开启动器");
  expect(app.texts().filter(t => t === "米哈游账号：10001").length).toBe(1);
  expect(app.texts()).toContain("已验证游戏 UID：123456789");
  await app.press("检查授权：10001");
  expect(app.texts()).toContain("账号 10001 米游社授权有效");
});

test("keeps the scan status while a replacement QR is being created", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("米哈游账号：10001"));
  await app.press("模拟延迟创建二维码");
  await app.press("重新扫码：10001");
  await app.press("检查授权：10001");
  expect(app.texts()).toContain("正在获取二维码…");
  await app.waitUntil(() => app.texts().includes("等待扫描：请使用米游社扫码"));
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

test("rejects another account during renewal and restores the original", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认新凭据");
  await app.waitUntil(() => app.texts().includes("米哈游账号：10001"));
  await app.press("模拟另一米哈游账号");
  await app.press("重新扫码：10001");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("扫码账号与待更新账号不一致，请使用原账号重新扫码"));
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("米哈游账号：10001");
  expect(app.texts()).toContain("已验证游戏 UID：123456789");
  expect(app.texts()).not.toContain("米哈游账号：10002");
  await app.press("检查授权：10001");
  expect(app.texts()).toContain("账号 10001 米游社授权有效");
});

test("restores the original authorization after interrupted renewal scans", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认新凭据");
  await app.waitFor("米游社授权已保存");

  for (const [action, status] of [
    ["取消扫码", "已取消扫码"],
    ["模拟手机取消", "已取消扫码"],
    ["模拟二维码过期", "二维码已过期，请刷新二维码"],
    ["模拟扫码网络失败", "网络失败：无法连接米哈游服务，请重试"],
  ]) {
    await app.press("重新扫码：10001");
    await app.press(action);
    await app.waitFor(status);
    await app.press("重新打开启动器");
    expect(app.texts()).toContain("米哈游账号：10001");
    expect(app.texts()).toContain("已验证游戏 UID：123456789");
    await app.press("检查授权：10001");
    expect(app.texts()).toContain("账号 10001 米游社授权有效");
  }
});

test("retains the original account and UID when renewal cannot verify the same UID", async () => {
  const app = await open(".", { clock: {} });
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认新凭据");
  await app.waitUntil(() => app.texts().includes("米游社授权已保存"));

  await app.press("模拟查不到游戏 UID");
  await app.press("重新扫码：10001");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("未查到该米哈游账号的游戏 UID"));
  await app.press("模拟归属 UID 改变");
  await app.press("重新扫码：10001");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("验证所得游戏 UID 与原授权不一致，已保留原授权"));
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("米哈游账号：10001");
  expect(app.texts()).toContain("已验证游戏 UID：123456789");
  expect(app.texts()).not.toContain("切换至 UID：987654321");
  await app.press("检查授权：10001");
  expect(app.texts()).toContain("账号 10001 米游社授权有效");
});

test("chooses a discovered installation and keeps the default after reopening", async () => {
  const app = await open(".");
  await app.press("选择：D:/原神");
  expect(app.texts()).toContain("默认目录：D:/原神");
  await app.press("重新打开启动器");
  expect(app.texts()).toContain("默认目录：D:/原神");
  expect(app.texts()).toContain("尚未保存米哈游账号");
  await app.press("普通启动");
  expect(app.texts()).toContain("已请求启动游戏");
  expect(app.texts()).toContain("请求启动：D:/原神");
});

test("starts the game independently of saved MiYouShe authorization", async () => {
  const app = await open(".", { clock: {} });
  await app.press("选择：D:/原神");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitUntil(() => app.texts().includes("米哈游账号：10001"));
  expect(app.texts()).toContain("米游社授权已保存");

  await app.press("检查授权：10001");
  expect(app.texts()).toContain("账号 10001 授权已失效，请重新扫码");
  await app.press("普通启动");
  expect(app.texts()).toContain("已请求启动游戏");
  expect(app.texts()).toContain("请求启动：D:/原神");
  expect(app.texts()).toContain("账号 10001 授权已失效，请重新扫码");
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
  expect(app.texts()).toContain("未找到游戏。可选择已有游戏目录，或下载国服官服。");
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
