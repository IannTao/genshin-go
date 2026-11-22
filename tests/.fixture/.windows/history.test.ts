import { expect, open, test } from "nsh/test";

test("Windows keyboard reaches history deletion in reading order, traps Tab, cancels on Enter/Esc, and restores focus", async () => {
  const app = await open("../nsh.json", { width: 1000, height: 960 });
  await app.press("角色资料");
  await app.type("game-uid", "1987654321");

  // Tab in reading order from game-uid -> 创建 UID 数据空间 -> 资料管理 -> 删除历史记录.
  await app.keyboard.press("tab");
  await app.keyboard.press("enter");
  await app.waitFor("当前游戏 UID：1987654321");
  await app.keyboard.press("tab");
  await app.keyboard.press("enter");
  await app.waitFor("删除历史记录");
  await app.keyboard.press("tab");

  // Leave the real dialog open so the Windows host handles initial Enter, Tab, and Escape.
  const pause = "等待 Windows 键盘输入";
  app.onDialog(() => { throw new Error(pause); });
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.dialogs()).toHaveLength(1);
  expect(app.dialogs()[0]).toMatchObject({
    title: "删除 UID 1987654321 的历史记录？",
    buttons: ["取消", "确认删除 UID：1987654321"],
  });

  // Initial Enter defaults to 取消 without pressing Tab first.
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  app.onDialog(() => "取消");
  await expect(app.wait(500)).rejects.toThrow("no dialog is shown");
  expect(app.texts()).not.toContain("已清除 UID 1987654321 的本机历史记录");

  // Focus returned to 删除历史记录, so Enter immediately reopens the same confirmation dialog.
  app.onDialog(() => { throw new Error(pause); });
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.dialogs()).toHaveLength(2);
  // Tab twice stays trapped inside the 2-button dialog and wraps back to 取消.
  await expect(app.keyboard.press("tab")).rejects.toThrow(pause);
  await expect(app.keyboard.press("tab")).rejects.toThrow(pause);
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  app.onDialog(() => "取消");
  await expect(app.wait(500)).rejects.toThrow("no dialog is shown");
  expect(app.texts()).not.toContain("已清除 UID 1987654321 的本机历史记录");

  // Focus returned again; even after Tab moves to the destructive button, Escape cancels safely.
  app.onDialog(() => { throw new Error(pause); });
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.dialogs()).toHaveLength(3);
  await expect(app.keyboard.press("tab")).rejects.toThrow(pause);
  await expect(app.keyboard.press("escape")).rejects.toThrow(pause);
  app.onDialog(() => "取消");
  await expect(app.wait(500)).rejects.toThrow("no dialog is shown");
  expect(app.texts()).not.toContain("已清除 UID 1987654321 的本机历史记录");

  // Focus returned to 删除历史记录 after Escape too; Tab + Enter confirms deletion.
  app.onDialog(() => { throw new Error(pause); });
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.dialogs()).toHaveLength(4);
  await expect(app.keyboard.press("tab")).rejects.toThrow(pause);
  await expect(app.keyboard.press("enter")).rejects.toThrow(pause);
  expect(app.texts()).toContain("已清除 UID 1987654321 的本机历史记录");
  app.onDialog(() => "取消");
  await expect(app.wait(500)).rejects.toThrow("no dialog is shown");
});
