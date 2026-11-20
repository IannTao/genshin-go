import { expect, open, test } from "nsh/test";

test("Windows Enter cancels history deletion from the initial dialog focus", async () => {
  const app = await open("../nsh.json", { width: 1000, height: 960 });
  await app.press("角色资料");
  await app.type("game-uid", "1987654321");
  await app.press("创建 UID 数据空间");
  await app.press("资料管理");

  // Leave the real dialog open so the Windows host handles the first Enter.
  const pause = "等待 Windows 键盘输入";
  app.onDialog(() => { throw new Error(pause); });
  await expect(app.press("删除历史记录")).rejects.toThrow(pause);
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);

  app.onDialog(() => "取消");
  await expect(app.wait(500)).rejects.toThrow("no dialog is shown");
  expect(app.texts()).not.toContain("已清除 UID 1987654321 的本机历史记录");
});
