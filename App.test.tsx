// The Rust root tested from TypeScript: `nsh test` runs this file in a
// headless runner it builds with the crate, and `open(".")` starts the root
// (`nsh.json` "root").
import { expect, open, test } from "nsh/test";
import { platform } from "process";

const windowsTest = platform === "windows" ? test : test.skip;

windowsTest("uses the selected UID's imported stock for character growth", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450231");
  await app.press("创建 UID 数据空间");
  await app.press("背包仓库");
  await app.type("inventory-file", "tests/data/inventory-good-growth.json");
  await app.press("读取 GOOD 文件");
  await app.press("预览背包导入至 UID 123450231");
  await app.press("备份并确认完整导入");
  await app.press("背包仓库");
  await app.press("角色养成计划");
  await app.type("growth-search", "安柏");
  await app.press("规划安柏");
  await app.clear("growth-target-level");
  await app.type("growth-target-level", "21");
  await app.clear("growth-target-phase");
  await app.type("growth-target-phase", "1");
  await app.type("growth-stock-100055", "2");
  await app.waitFor("小灯草 · 需求 3 · 缺口 1");

  await app.press("使用导入库存");
  await app.waitFor("导入库存采集时间：2026-10-11T08:00:00+08:00");
  expect(app.texts()).toContain("小灯草 · 需求 3 · 导入库存 9 · 缺口 0");
  expect(app.texts()).toContain("牢固的箭簇 · 需求 3 · 导入库存 0 · 缺口 3");
  expect(app.texts()).toContain("经验书折算 · 需求 136000 · 缺口 0");
  await app.press("保存养成计划");
  await app.waitFor("已保存安柏的养成计划");
  await app.press("使用手填库存");
  await app.waitFor("小灯草 · 需求 3 · 缺口 1");
  await app.press("关闭计划编辑");
  await app.press("查看安柏的计划（等级 1 → 21）");
  await app.waitFor("小灯草 · 需求 3 · 缺口 3");
  await app.press("使用导入库存");

  await app.type("game-uid", "123450232");
  await app.press("创建 UID 数据空间");
  await app.press("规划安柏");
  await app.waitFor("当前 UID 暂无可用的背包快照");
  expect(app.texts()).not.toContain("使用导入库存");
  expect(app.texts()).not.toContain("导入库存采集时间：2026-10-11T08:00:00+08:00");
}, 60000);

windowsTest("uses the latest complete imported stock for weapon growth", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450233");
  await app.press("创建 UID 数据空间");
  await app.press("武器养成计划");
  await app.type("weapon-growth-search", "13301");
  await app.press("规划白缨枪");
  await app.clear("weapon-growth-target-level");
  await app.type("weapon-growth-target-level", "21");
  await app.clear("weapon-growth-target-phase");
  await app.type("weapon-growth-target-phase", "1");

  await app.press("背包仓库");
  await app.type("inventory-file", "tests/data/inventory-good-newer.json");
  await app.press("读取 GOOD 文件");
  await app.press("预览背包导入至 UID 123450233");
  await app.press("备份并确认完整导入");
  await app.press("使用导入库存");
  await app.waitFor("导入库存采集时间：2026-10-10T07:50:00+08:00");
  expect(app.texts()).toContain("孤云寒林的光砂 · 需求 2 · 导入库存 0 · 缺口 2");

  await app.clear("inventory-file");
  await app.type("inventory-file", "tests/data/inventory-good-growth.json");
  await app.press("读取 GOOD 文件");
  await app.press("预览背包导入至 UID 123450233");
  await app.press("备份并确认完整导入");
  await app.waitFor("导入库存采集时间：2026-10-11T08:00:00+08:00");
  expect(app.texts()).toContain("孤云寒林的光砂 · 需求 2 · 导入库存 1 · 缺口 1");
  expect(app.texts()).toContain("摩拉 · 需求 11080 · 导入库存 30000 · 缺口 0");
  expect(app.texts()).toContain("强化矿折算 · 需求 60800 · 缺口 0");

  await app.clear("inventory-file");
  await app.type("inventory-file", "tests/data/genshinData_GOOD_2026_10_09_08_00.json");
  await app.press("读取 GOOD 文件");
  await app.type("inventory-captured-at", "+08:00");
  await app.press("预览背包导入至 UID 123450233");
  await app.waitFor("这份快照较旧，导入后可在历史中主动选择。");
  await app.press("备份并确认完整导入");
  expect(app.texts()).toContain("导入库存采集时间：2026-10-11T08:00:00+08:00");
  expect(app.texts()).toContain("孤云寒林的光砂 · 需求 2 · 导入库存 1 · 缺口 1");
}, 60000);

windowsTest("saves weapon growth and shares manual stock within the selected UID", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450224");
  await app.press("创建 UID 数据空间");
  await app.press("武器养成计划");
  await app.type("weapon-growth-search", "13301");
  await app.press("规划白缨枪");
  await app.clear("weapon-growth-target-level");
  await app.type("weapon-growth-target-level", "21");
  await app.waitFor("目标状态：此突破阶段的等级须为 1 至 20");
  await app.clear("weapon-growth-target-phase");
  await app.type("weapon-growth-target-phase", "1");
  expect(app.texts()).toContain("武器经验 · 需求 60400");
  expect(app.texts()).toContain("强化矿折算 · 需求 60800 · 缺口 60800");
  expect(app.texts()).toContain("摩拉 · 需求 11080 · 缺口 11080");
  expect(app.texts()).toContain("孤云寒林的光砂 · 需求 2 · 缺口 2");
  await app.type("weapon-growth-stock-114013", "1");
  await app.type("weapon-growth-stock-104012", "1");
  await app.type("weapon-growth-stock-202", "1000");
  expect(app.texts()).toContain("孤云寒林的光砂 · 需求 2 · 缺口 1");
  expect(app.texts()).toContain("强化矿折算 · 需求 60800 · 缺口 58800");
  await app.press("保存养成计划");
  await app.waitFor("已保存白缨枪的养成计划");

  await app.press("角色养成计划");
  await app.type("growth-search", "安柏");
  await app.press("规划安柏");
  await app.clear("growth-target-level");
  await app.type("growth-target-level", "2");
  expect(app.texts()).toContain("摩拉 · 需求 200 · 缺口 0");

  await app.type("game-uid", "123450225");
  await app.press("创建 UID 数据空间");
  await app.press("武器养成计划");
  await app.waitFor("暂无武器养成计划");
  await app.press("规划白缨枪");
  await app.clear("weapon-growth-target-level");
  await app.type("weapon-growth-target-level", "2");
  expect(app.texts()).toContain("强化矿折算 · 需求 400 · 缺口 400");
  await app.press("保存养成计划");
  await app.waitFor("已保存白缨枪的养成计划");

  const reopened = await open(".", { width: 1024, height: 1000 });
  await reopened.press("角色资料");
  await reopened.press("武器养成计划");
  await reopened.press("查看白缨枪的计划（等级 1 → 2）");
  expect(reopened.texts()).toContain("强化矿折算 · 需求 400 · 缺口 400");
  await reopened.press("切换至 UID：123450224");
  await reopened.press("查看白缨枪的计划（等级 1 → 21）");
  expect(reopened.texts()).toContain("强化矿折算 · 需求 60800 · 缺口 58800");
  expect(reopened.texts()).toContain("孤云寒林的光砂 · 需求 2 · 缺口 1");
  await reopened.clear("weapon-growth-current-level");
  await reopened.type("weapon-growth-current-level", "20");
  await reopened.press("保存养成计划");
  await reopened.waitFor("查看白缨枪的计划（等级 20 → 21）");
  expect(reopened.texts()).toContain("摩拉 · 需求 5720 · 缺口 4720");

  await reopened.press("资料管理");
  reopened.onDialog(() => "确认删除 UID：123450224");
  await reopened.press("删除历史记录");
  await reopened.waitFor("暂无武器养成计划");
  await reopened.clear("weapon-growth-search");
  await reopened.type("weapon-growth-search", "13301");
  await reopened.press("规划白缨枪");
  await reopened.clear("weapon-growth-target-level");
  await reopened.type("weapon-growth-target-level", "2");
  expect(reopened.texts()).toContain("摩拉 · 需求 40 · 缺口 40");
  await reopened.press("切换至 UID：123450225");
  await reopened.press("查看白缨枪的计划（等级 1 → 2）");
}, 60000);

windowsTest("applies the four ascension phases of a one-star weapon", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450226");
  await app.press("创建 UID 数据空间");
  await app.press("武器养成计划");
  await app.type("weapon-growth-search", "无锋剑");
  await app.press("规划无锋剑");
  await app.clear("weapon-growth-current-level");
  await app.type("weapon-growth-current-level", "60");
  await app.clear("weapon-growth-current-phase");
  await app.type("weapon-growth-current-phase", "3");
  await app.clear("weapon-growth-target-level");
  await app.type("weapon-growth-target-level", "60");
  await app.clear("weapon-growth-target-phase");
  await app.type("weapon-growth-target-phase", "4");
  expect(app.texts()).toContain("高塔孤王的断片 · 需求 1 · 缺口 1");
  expect(app.texts()).toContain("摩拉 · 需求 10000 · 缺口 10000");
  await app.clear("weapon-growth-target-level");
  await app.type("weapon-growth-target-level", "71");
  await app.waitFor("目标状态：此突破阶段的等级须为 60 至 70");
  await app.clear("weapon-growth-target-phase");
  await app.type("weapon-growth-target-phase", "5");
  await app.waitFor("目标状态：突破阶段须为 0 至 4");
}, 60000);

windowsTest("plans the Exaiphanes Blade's quest levels without enhancement materials", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450227");
  await app.press("创建 UID 数据空间");
  await app.press("武器养成计划");
  await app.type("weapon-growth-search", "星锋剑");
  await app.press("规划星锋剑");
  await app.clear("weapon-growth-target-phase");
  await app.type("weapon-growth-target-phase", "2");
  await app.waitFor("目标状态：此阶段的等级须为 40");
  await app.clear("weapon-growth-target-level");
  await app.type("weapon-growth-target-level", "40");
  await app.waitFor("无需养成材料");
  await app.press("保存养成计划");
  await app.waitFor("查看星锋剑的计划（等级 1 → 40）");
}, 60000);

windowsTest("counts experience-book rounding at ascension caps", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450223");
  await app.press("创建 UID 数据空间");
  await app.press("角色养成计划");
  await app.type("growth-search", "安柏");
  await app.press("规划安柏");
  await app.clear("growth-target-level");
  await app.type("growth-target-level", "22");
  await app.clear("growth-target-phase");
  await app.type("growth-target-phase", "1");
  await app.type("growth-stock-104001", "152");
  expect(app.texts()).toContain("角色经验 · 需求 151850");
  expect(app.texts()).toContain("经验书折算 · 需求 153000 · 缺口 1000");
  expect(app.texts()).toContain("摩拉 · 需求 50600 · 缺口 50600");

  await app.type("growth-stock-100055", "invalid");
  await app.clear("growth-target-level");
  await app.type("growth-target-level", "2");
  await app.clear("growth-target-phase");
  await app.type("growth-target-phase", "0");
  await app.press("保存养成计划");
  await app.waitFor("已保存安柏的养成计划");
}, 60000);

windowsTest("saves a character plan and manual material stock for the selected UID", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450221");
  await app.press("创建 UID 数据空间");
  await app.press("角色养成计划");
  await app.type("growth-search", "安柏");
  await app.press("规划安柏");
  await app.clear("growth-current-level");
  await app.type("growth-current-level", "20");
  await app.clear("growth-target-level");
  await app.type("growth-target-level", "20");
  await app.clear("growth-target-phase");
  await app.type("growth-target-phase", "1");
  expect(app.texts()).toContain("小灯草 · 需求 3 · 缺口 3");
  await app.type("growth-stock-100055", "2");
  expect(app.texts()).toContain("小灯草 · 需求 3 · 缺口 1");
  await app.press("保存养成计划");
  await app.waitFor("已保存安柏的养成计划");

  await app.type("game-uid", "123450222");
  await app.press("创建 UID 数据空间");
  await app.waitFor("暂无角色养成计划");
  await app.press("规划安柏");
  await app.clear("growth-target-level");
  await app.type("growth-target-level", "2");
  await app.waitFor("经验书折算 · 需求 1000 · 缺口 1000");
  await app.type("growth-stock-104001", "1");
  await app.waitFor("经验书折算 · 需求 1000 · 缺口 0");
  await app.press("保存养成计划");

  const reopened = await open(".", { width: 1024, height: 1000 });
  await reopened.press("角色资料");
  await reopened.press("角色养成计划");
  await reopened.press("查看安柏的计划（等级 1 → 2）");
  expect(reopened.texts()).toContain("经验书折算 · 需求 1000 · 缺口 0");
  await reopened.press("切换至 UID：123450221");
  await reopened.press("查看安柏的计划（等级 20 → 20）");
  expect(reopened.texts()).toContain("小灯草 · 需求 3 · 缺口 1");
  await reopened.clear("growth-current-level");
  await reopened.type("growth-current-level", "40");
  await reopened.clear("growth-current-phase");
  await reopened.type("growth-current-phase", "1");
  await reopened.clear("growth-target-level");
  await reopened.type("growth-target-level", "41");
  await reopened.clear("growth-target-phase");
  await reopened.type("growth-target-phase", "2");
  await reopened.clear("growth-target-normal");
  await reopened.type("growth-target-normal", "2");
  expect(reopened.texts()).toContain("小灯草 · 需求 10 · 缺口 8");
  expect(reopened.texts()).toContain("牢固的箭簇 · 需求 21 · 缺口 21");
  expect(reopened.texts()).toContain("「自由」的教导 · 需求 3 · 缺口 3");
  expect(reopened.texts()).toContain("摩拉 · 需求 61900 · 缺口 61900");
  await reopened.press("保存养成计划");
  await reopened.waitFor("查看安柏的计划（等级 40 → 41）");

  await reopened.press("资料管理");
  reopened.onDialog(() => "确认删除 UID：123450221");
  await reopened.press("删除历史记录");
  await reopened.waitFor("暂无角色养成计划");
  await reopened.type("growth-search", "安柏");
  await reopened.press("规划安柏");
  await reopened.clear("growth-target-level");
  await reopened.type("growth-target-level", "20");
  await reopened.clear("growth-target-phase");
  await reopened.type("growth-target-phase", "1");
  expect(reopened.texts()).toContain("小灯草 · 需求 3 · 缺口 3");
  await reopened.press("切换至 UID：123450222");
  await reopened.press("查看安柏的计划（等级 1 → 2）");
  expect(reopened.texts()).toContain("经验书折算 · 需求 1000 · 缺口 0");
}, 60000);

windowsTest("previews and backs up a complete inventory before importing it for the chosen UID", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450120");
  await app.press("创建 UID 数据空间");
  await app.press("背包仓库");
  await app.type("inventory-file", "tests/data/genshinData_GOOD_2026_10_09_08_00.json");
  await app.press("读取 GOOD 文件");
  await app.type("inventory-captured-at", "+08:00");
  await app.press("预览背包导入至 UID 123450120");
  expect(app.texts()).toContain("UID 123450120 · 武器 1 件 · 圣遗物 1 件 · 材料 1 种");
  expect(app.texts()).toContain("暂无背包快照");
  await app.press("备份并确认完整导入");
  expect(app.texts().join("\n")).toContain("导入前备份：");
  expect(app.texts()).toContain("采集时间：2026-10-09T08:00:00+08:00");
  expect(app.texts().join("\n")).toContain("DullBlade · 等级 20");
  await app.press("圣遗物");
  expect(app.texts().join("\n")).toContain("GladiatorsFinale");
  await app.press("材料");
  expect(app.texts()).toContain("SlimeCondensate · 数量 81");

  await app.clear("inventory-file");
  await app.type("inventory-file", "tests/data/inventory-good-newer.json");
  await app.press("读取 GOOD 文件");
  await app.press("预览背包导入至 UID 123450120");
  await app.press("备份并确认完整导入");
  await app.press("材料");
  expect(app.texts()).toContain("SlimeCondensate · 数量 3");
  const result = app.texts().find((line) => line.startsWith("已导入 UID 123450120 的背包快照。"));
  expect(result).toBeDefined();
  const backup = result?.split("导入前备份：")[1] ?? "";
  await app.clear("inventory-file");
  await app.type("inventory-file", backup);
  await app.press("读取 GOOD 文件");
  expect(app.texts()).toContain("文件采集时间：2026-10-09T08:00:00+08:00");
  await app.press("预览背包导入至 UID 123450120");
  expect(app.texts()).toContain("这份背包快照已保存。");
  expect(app.texts()).not.toContain("备份并确认完整导入");
}, 60000);

windowsTest("imports a partial GOOD inventory and keeps omitted material counts after reopening", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450125");
  await app.press("创建 UID 数据空间");
  await app.press("背包仓库");
  await app.type("inventory-file", "tests/data/inventory-good-newer.json");
  await app.press("读取 GOOD 文件");
  await app.press("预览背包导入至 UID 123450125");
  await app.press("备份并确认完整导入");

  await app.clear("inventory-file");
  await app.type("inventory-file", "tests/data/inventory-good-scanner.json");
  await app.press("读取 GOOD 文件");
  expect(app.texts()).toContain("文件包含：武器、圣遗物。未提供的类别保留本机记录。");
  await app.press("预览背包导入至 UID 123450125");
  await app.press("备份并确认导入所含类别");

  const reopened = await open(".", { width: 1024, height: 1000 });
  await reopened.press("角色资料");
  if (!reopened.texts().includes("当前游戏 UID：123450125")) {
    await reopened.press("切换至 UID：123450125");
  }
  await reopened.press("背包仓库");
  expect(reopened.texts().join("\n")).toContain("DullBlade · 等级 20");
  await reopened.press("圣遗物");
  expect(reopened.texts()).toContain("此快照无圣遗物记录");
  await reopened.press("材料");
  expect(reopened.texts()).toContain("SlimeCondensate · 数量 3");

  await reopened.type("inventory-file", "tests/data/inventory-good-kamera-materials.json");
  await reopened.press("读取 GOOD 文件");
  await reopened.press("预览背包导入至 UID 123450125");
  expect(reopened.texts()).toContain("UID 123450125 · 材料 1 种");
  await reopened.press("备份并确认导入所含类别");
  await reopened.press("材料");
  expect(reopened.texts()).toContain("SlimeCondensate · 数量 4");
  await reopened.press("武器");
  expect(reopened.texts().join("\n")).toContain("DullBlade · 等级 20");

  await reopened.press("背包快照历史");
  await reopened.press("查看旧快照：2026-11-01T08:00:00+08:00");
  await reopened.press("材料");
  expect(reopened.texts()).toContain("正在查看旧快照");
  expect(reopened.texts()).toContain("采集时间：2026-11-01T08:00:00+08:00");
  expect(reopened.texts()).toContain("此旧快照未包含材料记录");

  await reopened.clear("inventory-file");
  await reopened.type("inventory-file", "tests/data/genshin_export_2026-11-03_08-00-00.json");
  await reopened.press("读取 GOOD 文件");
  await reopened.type("inventory-captured-at", "+08:00");
  await reopened.press("预览背包导入至 UID 123450125");
  expect(reopened.texts()).toContain("待导入采集时间：2026-11-03T08:00:00+08:00");

  await reopened.clear("inventory-file");
  await reopened.type("inventory-file", "tests/data/genshinData_GOOD_2026_10_09_08_00.json");
  await reopened.press("读取 GOOD 文件");
  await reopened.clear("inventory-captured-at");
  await reopened.type("inventory-captured-at", "2026-11-04T08:00:00+08:00");
  await reopened.press("预览背包导入至 UID 123450125");
  expect(reopened.texts()).toContain(
    "目标 UID 当前相关快照：2026-11-02T08:00:00+08:00（材料）、2026-11-01T08:00:00+08:00（武器、圣遗物）",
  );
  await reopened.press("备份并确认完整导入");
  const multiResult = reopened.texts().find((line) => line.startsWith("已导入 UID 123450125 的背包快照。"));
  expect(multiResult).toBeDefined();
  const backups = (multiResult?.split("导入前备份：")[1] ?? "").split("、");
  expect(backups.length).toBe(2);
  for (const backupPath of backups) {
    await reopened.clear("inventory-file");
    await reopened.type("inventory-file", backupPath);
    await reopened.press("读取 GOOD 文件");
    await reopened.press("预览背包导入至 UID 123450125");
    expect(reopened.texts()).toContain("这份背包快照已保存。");
  }
}, 60000);

windowsTest("keeps older inventory selectable and reopens separate UID inventories", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450121");
  await app.press("创建 UID 数据空间");
  await app.press("背包仓库");
  await app.type("inventory-file", "tests/data/inventory-good-newer.json");
  await app.press("读取 GOOD 文件");
  await app.press("预览背包导入至 UID 123450121");
  await app.press("备份并确认完整导入");

  await app.clear("inventory-file");
  await app.type("inventory-file", "tests/data/genshinData_GOOD_2026_10_09_08_00.json");
  await app.press("读取 GOOD 文件");
  await app.type("inventory-captured-at", "+08:00");
  await app.press("预览背包导入至 UID 123450121");
  expect(app.texts()).toContain("这份快照较旧，导入后可在历史中主动选择。");
  await app.press("备份并确认完整导入");
  expect(app.texts()).toContain("采集时间：2026-10-10T07:50:00+08:00");
  await app.press("背包快照历史");
  await app.press("查看旧快照：2026-10-09T08:00:00+08:00");
  expect(app.texts()).toContain("正在查看旧快照");
  await app.press("材料");
  expect(app.texts()).toContain("SlimeCondensate · 数量 81");

  await app.type("game-uid", "123450122");
  await app.press("创建 UID 数据空间");
  await app.press("预览背包导入至 UID 123450122");
  await app.press("备份并确认完整导入");
  const reopened = await open(".", { width: 1024, height: 1000 });
  await reopened.press("角色资料");
  await reopened.press("背包仓库");
  await reopened.press("材料");
  expect(reopened.texts()).toContain("UID 123450122 的背包仓库");
  expect(reopened.texts()).toContain("SlimeCondensate · 数量 81");
  await reopened.press("切换至 UID：123450121");
  await reopened.press("材料");
  expect(reopened.texts()).toContain("采集时间：2026-10-10T07:50:00+08:00");
  expect(reopened.texts()).toContain("SlimeCondensate · 数量 3");
  await reopened.press("资料管理");
  reopened.onDialog(() => "确认删除 UID：123450121");
  await reopened.press("删除历史记录");
  await reopened.waitFor("暂无背包快照");
  await reopened.press("切换至 UID：123450122");
  await reopened.press("材料");
  expect(reopened.texts()).toContain("SlimeCondensate · 数量 81");
}, 60000);

windowsTest("uses the incomplete-category confirmation and preserves the UID and time in GOOD v3 inventory", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450123");
  await app.press("创建 UID 数据空间");
  await app.type("game-uid", "123450124");
  await app.press("创建 UID 数据空间");
  await app.press("背包仓库");
  await app.type("inventory-file", "tests/data/inventory-good-kamera-empty-artifacts.json");
  await app.press("读取 GOOD 文件");
  await app.press("预览背包导入至 UID 123450123");
  expect(app.texts()).toContain("UID 123450123 · 武器 1 件 · 材料 1 种");
  await app.press("确认缺失类别的库存为空");
  await app.press("预览背包导入至 UID 123450123");
  expect(app.texts()).toContain("UID 123450123 · 武器 1 件 · 圣遗物 0 件 · 材料 1 种");

  await app.clear("inventory-file");
  await app.type("inventory-file", "tests/data/inventory-good-leyline.json");
  await app.press("读取 GOOD 文件");
  expect(app.texts()).toContain("文件声明游戏 UID：123450123");
  expect(app.texts()).toContain("文件采集时间：2026-10-14T19:06:40.123Z");
  expect(app.texts()).not.toContain("预览背包导入至 UID 123450124");
  await app.press("预览背包导入至 UID 123450123");
  await app.press("备份并确认完整导入");
  expect(app.texts()).toContain("暂无背包快照");
  await app.press("切换至 UID：123450123");
  expect(app.texts()).toContain("采集时间：2026-10-14T19:06:40.123Z");
  await app.press("圣遗物");
  expect(app.texts().join("\n")).toContain("GladiatorsFinale");

  await app.clear("inventory-file");
  await app.type("inventory-file", "tests/data/inventory-good-duplicate-material.json");
  await app.press("读取 GOOD 文件");
  expect(app.texts().join("\n")).toContain("材料标识重复：Mora");
  expect(app.texts()).not.toContain("预览背包导入至 UID 123450123");

  await app.clear("inventory-file");
  await app.type("inventory-file", "tests/data/inventory-good-leyline-partial.json");
  await app.press("读取 GOOD 文件");
  expect(app.texts()).toContain("文件包含未完整共享的背包数据，请导出完整的武器、圣遗物和材料。");
  expect(app.texts()).not.toContain("预览背包导入至 UID 123450123");
}, 60000);

windowsTest("restores all portable UID data from an overall backup", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450020");
  await app.press("创建 UID 数据空间");
  await app.type("uigf-file", "tests/data/uigf-v4.2.json");
  await app.press("读取 UIGF 文件");
  await app.press("备份并确认导入");
  const wish = "2025-10-09 08:00:00 · 角色活动祈愿 · 神里绫华 · 5 星";
  await app.waitFor(wish);
  await app.press("成就");
  await app.press("标记完成：风与异乡人（80001）");

  await app.press("角色资料");
  await app.press("角色养成计划");
  await app.type("growth-search", "安柏");
  await app.press("规划安柏");
  await app.clear("growth-target-level");
  await app.type("growth-target-level", "2");
  await app.type("growth-stock-104001", "1");
  await app.press("保存养成计划");
  await app.waitFor("已保存安柏的养成计划");
  await app.press("武器养成计划");
  await app.type("weapon-growth-search", "13301");
  await app.press("规划白缨枪");
  await app.clear("weapon-growth-target-level");
  await app.type("weapon-growth-target-level", "3");
  await app.press("保存养成计划");
  await app.waitFor("已保存白缨枪的养成计划");

  await app.press("背包仓库");
  await app.type("inventory-file", "tests/data/genshinData_GOOD_2026_10_09_08_00.json");
  await app.press("读取 GOOD 文件");
  await app.type("inventory-captured-at", "+08:00");
  await app.press("预览背包导入至 UID 123450020");
  await app.press("备份并确认完整导入");

  await app.press("备份与恢复");
  await app.press("创建整体备份");
  const backup = app.texts().find((line) => line.startsWith("已创建整体备份："));
  expect(backup).toBeDefined();
  const path = backup?.slice("已创建整体备份：".length) ?? "";

  await app.press("资料管理");
  app.onDialog(() => "确认删除 UID：123450020");
  await app.press("删除历史记录");
  await app.waitFor("已清除 UID 123450020 的本机历史记录");

  await app.clear("inventory-file");
  await app.type("inventory-file", "tests/data/inventory-good-newer.json");
  await app.press("读取 GOOD 文件");
  await app.press("预览背包导入至 UID 123450020");
  await app.press("备份并确认完整导入");

  await app.type("restore-file", path);
  await app.press("预览恢复");
  expect(app.texts()).toContain("UID 123450020 · 祈愿新增 1，重复 0，冲突 0");
  expect(app.texts()).toContain("UID 123450020 · 成就新增 1，更新 0，重复 0，冲突 0");
  expect(app.texts()).toContain("UID 123450020 · 角色计划新增 1，重复 0，冲突 0");
  expect(app.texts()).toContain("UID 123450020 · 武器计划新增 1，重复 0，冲突 0");
  expect(app.texts()).toContain("UID 123450020 · 背包快照新增 1，重复 0，冲突 0");
  expect(app.texts()).toContain("恢复后默认采集时间：2026-10-10T07:50:00+08:00");
  expect(app.texts()).toContain("暂无祈愿记录");
  expect(app.texts()).toContain("暂无武器养成计划");
  expect(app.texts()).toContain("采集时间：2026-10-10T07:50:00+08:00");
  await app.press("成就");
  expect(app.texts()).toContain("风与异乡人 · 成就 80001 · 未完成");
  await app.press("角色资料");
  await app.press("备份并确认恢复");
  expect(app.texts().join("\n")).toContain("恢复完成。恢复前备份：");
  await app.waitFor(wish);
  await app.press("查看白缨枪的计划（等级 1 → 3）");
  await app.press("角色养成计划");
  await app.press("查看安柏的计划（等级 1 → 2）");
  expect(app.texts()).toContain("经验书折算 · 需求 1000 · 缺口 0");
  await app.press("材料");
  expect(app.texts()).toContain("采集时间：2026-10-10T07:50:00+08:00");
  expect(app.texts()).toContain("SlimeCondensate · 数量 3");
  await app.press("背包快照历史");
  await app.press("查看旧快照：2026-10-09T08:00:00+08:00");
  expect(app.texts()).toContain("采集时间：2026-10-09T08:00:00+08:00");
  await app.press("材料");
  expect(app.texts()).toContain("SlimeCondensate · 数量 81");
  await app.press("武器");
  expect(app.texts().join("\n")).toContain("DullBlade · 等级 20");
  await app.press("圣遗物");
  expect(app.texts().join("\n")).toContain("GladiatorsFinale");
  await app.press("预览恢复");
  expect(app.texts()).toContain("UID 123450020 · 祈愿新增 0，重复 1，冲突 0");
  expect(app.texts()).toContain("没有需要恢复的变更");
  expect(app.texts()).not.toContain("备份并确认恢复");
  await app.press("成就");
  expect(app.texts()).toContain("风与异乡人 · 成就 80001 · 已完成 · 进度未记录");
}, 60000);

windowsTest("requires an explicit version for each conflicting growth plan or manual quantity", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.type("game-uid", "123450024");
  await app.press("创建 UID 数据空间");
  await app.press("角色养成计划");
  await app.type("growth-search", "安柏");
  await app.press("规划安柏");
  await app.clear("growth-target-level");
  await app.type("growth-target-level", "2");
  await app.type("growth-stock-104001", "1");
  await app.press("保存养成计划");
  await app.press("武器养成计划");
  await app.type("weapon-growth-search", "13301");
  await app.press("规划白缨枪");
  await app.clear("weapon-growth-target-level");
  await app.type("weapon-growth-target-level", "2");
  await app.press("保存养成计划");
  await app.press("备份与恢复");
  await app.press("创建整体备份");
  const prefix = "已创建整体备份：";
  const path = app.texts().find((line) => line.startsWith(prefix))?.slice(prefix.length) ?? "";
  expect(path.length).toBeGreaterThan(0);

  await app.clear("weapon-growth-target-level");
  await app.type("weapon-growth-target-level", "3");
  await app.press("保存养成计划");
  await app.press("角色养成计划");
  await app.press("查看安柏的计划（等级 1 → 2）");
  await app.clear("growth-target-level");
  await app.type("growth-target-level", "3");
  await app.clear("growth-stock-104001");
  await app.type("growth-stock-104001", "0");
  await app.press("保存养成计划");
  await app.waitFor("查看安柏的计划（等级 1 → 3）");

  await app.type("restore-file", path);
  await app.press("预览恢复");
  expect(app.texts()).toContain("UID 123450024 · 角色计划新增 0，重复 0，冲突 1");
  expect(app.texts()).toContain("UID 123450024 · 武器计划新增 0，重复 0，冲突 1");
  expect(app.texts()).not.toContain("备份并确认恢复");
  await app.press("使用备份 · UID 123450024 · 角色计划 安柏");
  await app.press("保留本机 · UID 123450024 · 武器计划 白缨枪");
  expect(app.texts()).not.toContain("备份并确认恢复");
  await app.press("使用备份 · UID 123450024 · 手填库存 流浪者的经验（材料 104001）");
  await app.press("备份并确认恢复");
  await app.press("查看安柏的计划（等级 1 → 2）");
  expect(app.texts()).toContain("经验书折算 · 需求 1000 · 缺口 0");
  await app.press("武器养成计划");
  await app.waitFor("查看白缨枪的计划（等级 1 → 3）");
}, 60000);

windowsTest("previews and creates a missing UID from an overall backup", async () => {
  const app = await open(".", { width: 1024, height: 1000 });
  await app.press("角色资料");
  await app.press("备份与恢复");
  await app.type("restore-file", "tests/data/uid-backup-v1.json");
  await app.press("预览恢复");
  await app.waitFor("新建 UID 数据空间：123450022");
  expect(app.texts()).not.toContain("切换至 UID：123450022");
  expect(app.texts()).not.toContain("当前游戏 UID：123450022");
  await app.press("备份并确认恢复");

  if (app.texts().includes("切换至 UID：123450022")) {
    await app.press("切换至 UID：123450022");
  }
  await app.waitFor("当前游戏 UID：123450022");
});

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

windowsTest("saves and clears manual achievement progress by UID without authorization", async () => {
  const app = await open(".");
  await app.press("角色资料");
  await app.type("game-uid", "123450017");
  await app.press("创建 UID 数据空间");
  await app.press("成就");
  expect(app.texts()).toContain("成就目录 · 原神 7.2");
  expect(app.texts()).toContain("风与异乡人 · 成就 80001 · 未完成");
  await app.press("标记完成：风与异乡人（80001）");
  expect(app.texts()).toContain("风与异乡人 · 成就 80001 · 已完成 · 进度未记录");

  await app.press("角色资料");
  await app.type("game-uid", "123450117");
  await app.press("创建 UID 数据空间");
  const reopened = await open(".");
  await reopened.press("成就");
  expect(reopened.texts()).toContain("UID 123450117 的成就进度");
  expect(reopened.texts()).toContain("风与异乡人 · 成就 80001 · 未完成");
  await reopened.press("标记完成：风与异乡人（80001）");
  await reopened.press("角色资料");
  await reopened.press("切换至 UID：123450017");
  await reopened.press("成就");
  expect(reopened.texts()).toContain("风与异乡人 · 成就 80001 · 已完成 · 进度未记录");
  await reopened.press("取消完成：风与异乡人（80001）");
  expect(reopened.texts()).toContain("风与异乡人 · 成就 80001 · 未完成 · 进度未记录");
  await reopened.press("标记完成：风与异乡人（80001）");

  await reopened.press("角色资料");
  await reopened.press("资料管理");
  reopened.onDialog(() => "确认删除 UID：123450017");
  await reopened.press("删除历史记录");
  await reopened.waitFor("已清除 UID 123450017 的本机历史记录");
  await reopened.press("成就");
  expect(reopened.texts()).toContain("风与异乡人 · 成就 80001 · 未完成");
  await reopened.press("角色资料");
  await reopened.press("切换至 UID：123450117");
  await reopened.press("成就");
  expect(reopened.texts()).toContain("风与异乡人 · 成就 80001 · 已完成 · 进度未记录");
});

windowsTest("browses the achievement catalog and searches by name or ID", async () => {
  const app = await open(".");
  await app.press("角色资料");
  await app.type("game-uid", "123450217");
  await app.press("创建 UID 数据空间");
  await app.press("成就");
  await app.press("下一页");
  expect(app.texts()).toContain("已完成 0 项 · 共 1909 项 · 第 2/64 页");
  await app.type("achievement-search", "寒冬的冒险家");
  expect(app.texts()).toContain("寒冬的冒险家·其二 · 第 1 阶段 · 目标 3 · 成就 86102 · 未完成");
  expect(app.texts()).toContain("寒冬的冒险家·其二 · 第 2 阶段 · 目标 7 · 成就 86103 · 未完成");
  await app.clear("achievement-search");
  await app.type("achievement-search", "86102");
  expect(app.texts()).toContain("已完成 0 项 · 共 1 项 · 第 1/1 页");
  expect(app.texts()).toContain("寒冬的冒险家·其二 · 第 1 阶段 · 目标 3 · 成就 86102 · 未完成");
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
  expect(app.texts()).toContain("风与异乡人 · 成就 80001 · 已领取 · 进度 1");
  expect(app.texts()).toContain("千嶂万仞 · 成就 80002 · 未完成 · 进度 30");
  await app.press("预览导入至 UID 123450018");
  expect(app.texts()).toContain("导入至 UID 123450018：新增 0，更新 0，无变化 2");
  expect(app.texts()).toContain("重复 2，冲突 0（冲突按状态与进度自动合并）");
  expect(app.texts()).not.toContain("备份并确认导入");
  await app.press("导出 UIAF v1.1");
  expect(app.texts().join("\n")).toContain("UID 123450018 的 UIAF v1.1 已导出：");
  await app.press("标记完成：千嶂万仞（80002）");
  expect(app.texts()).toContain("千嶂万仞 · 成就 80002 · 已完成 · 进度 30");
});

windowsTest("browses the curated plugin catalog from the Extensions page", async () => {
  const app = await open(".", { width: 1024, height: 900 });
  await app.press("扩展");
  const texts = app.texts();
  expect(texts).toContain("本项目维护的精选插件目录，仅展示已核实条目，不开放任意第三方商店源。");
  expect(texts).toContain("精选目录（共 1 项）");
  expect(texts).toContain("条目详情：FufuLauncher.UnlockerIsland");
  expect(texts).toContain("类型：游戏注入模块");
  expect(texts).toContain("版本：1830f9d7（2026-11-18 固定提交）");
  expect(texts).toContain("来源：https://github.com/FufuLauncher/FufuLauncher.UnlockerIsland");
  expect(texts.some(line => line.startsWith("安装位置：") && line.endsWith("fufu-unlocker-island"))).toBe(true);
  expect(texts.some(line => line.startsWith("权限与风险：") && line.includes("管理员权限"))).toBe(true);
  expect(texts.some(line => line.startsWith("兼容格式：") && line.includes("Launcher_2.exe"))).toBe(true);
  expect(texts.some(line => line.startsWith("许可证：") && line.includes("AGPL-3.0"))).toBe(true);
});
