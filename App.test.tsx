// The Rust root tested from TypeScript: `nsh test` runs this file in a
// headless runner it builds with the crate, and `open(".")` starts the root
// (`nsh.json` "root").
import { expect, open, test } from "nsh/test";

test("opens the launcher home and reaches installation controls", async () => {
  const app = await open(".");
  expect(app.texts()).toContain("原神启动器");
  await app.press("管理游戏");
  expect(app.texts()).toContain("选择游戏目录");
  expect(app.texts()).not.toContain("普通启动");
});
