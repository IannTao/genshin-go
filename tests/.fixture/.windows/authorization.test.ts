import { expect, open, test } from "nsh/test";

test("Windows keyboard reaches removal in reading order, traps Tab, cancels on Enter/Esc, and restores focus", async () => {
  const app = await open("../nsh.json", { width: 1000, height: 960 });
  await app.press("米游社");
  await app.press("扫码添加米哈游账号");
  await app.press("模拟扫码确认");
  await app.waitFor("米游社授权已保存");
  await app.press("管理授权");

  // Tab in reading order from 管理授权 -> 检查授权：10001 -> 移除账号：10001.
  await app.keyboard.press("tab");
  await app.keyboard.press("tab");

  // Pause NSH's automatic dialog answer so the real Windows dialog receives keyboard input.
  const pause = "等待 Windows 键盘输入";
  app.onDialog(() => { throw new Error(pause); });
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.dialogs()).toHaveLength(1);
  expect(app.dialogs()[0]).toMatchObject({
    title: "移除米哈游账号 10001？",
    buttons: ["取消", "确认移除账号：10001"],
  });

  // Initial Enter defaults to 取消 without pressing Tab first.
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.texts()).toContain("米哈游账号：10001");
  app.onDialog(() => "取消");
  await expect(app.wait(500)).rejects.toThrow("no dialog is shown");

  // Focus returned to 移除账号：10001, so Enter immediately reopens the same confirmation dialog.
  app.onDialog(() => { throw new Error(pause); });
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.dialogs()).toHaveLength(2);
  // Tab twice stays trapped inside the 2-button dialog (取消 -> 确认移除 -> 取消).
  await expect(app.keyboard.press("tab")).rejects.toThrow(pause);
  await expect(app.keyboard.press("tab")).rejects.toThrow(pause);
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.texts()).toContain("米哈游账号：10001");
  app.onDialog(() => "取消");
  await expect(app.wait(500)).rejects.toThrow("no dialog is shown");

  // Focus returned again; even after Tab moves to the destructive button, Escape cancels safely.
  app.onDialog(() => { throw new Error(pause); });
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.dialogs()).toHaveLength(3);
  await expect(app.keyboard.press("tab")).rejects.toThrow(pause);
  await expect(app.keyboard.press("escape")).rejects.toThrow(pause);
  expect(app.texts()).toContain("米哈游账号：10001");
  app.onDialog(() => "取消");
  await expect(app.wait(500)).rejects.toThrow("no dialog is shown");

  // Focus returned to 移除账号：10001 after Escape too; Tab + Enter confirms removal.
  app.onDialog(() => { throw new Error(pause); });
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.dialogs()).toHaveLength(4);
  await expect(app.keyboard.press("tab")).rejects.toThrow(pause);
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.texts()).toContain("尚未保存米哈游账号");
  expect(app.texts()).toContain("已移除米游社授权，已保留 UID 数据");
  app.onDialog(() => "取消");
  await expect(app.wait(500)).rejects.toThrow("no dialog is shown");
});
