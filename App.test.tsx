// The Rust root tested from TypeScript: `nsh test` runs this file in a
// headless runner it builds with the crate, and `open(".")` starts the root
// (`nsh.json` "root").
import { expect, open, test } from "nsh/test";

test("shows installation selection and ordinary launch", async () => {
  const app = await open(".");
  expect(app.texts()).toContain("原神启动器");
  expect(app.texts()).toContain("选择游戏目录");
  expect(app.texts()).toContain("普通启动");
});
