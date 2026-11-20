import { expect, open, test } from "nsh/test";

test("Windows Enter cancels removal from the initial dialog focus", async () => {
  const app = await open("../nsh.json", { width: 1000, height: 960 });
  await app.press("米游社");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitFor("米游社授权已保存");
  await app.press("管理授权");

  // Pause NSH's automatic dialog answer so the Windows host receives Enter first.
  const pause = "等待 Windows 键盘输入";
  app.onDialog(() => { throw new Error(pause); });
  await expect(app.press("移除账号：10001")).rejects.toThrow(pause);
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.texts()).toContain("米哈游账号：10001");

  // NSH still tracks the dialog until it answers. The native dialog should be gone.
  app.onDialog(() => "取消");
  await expect(app.wait(500)).rejects.toThrow("no dialog is shown");
});
