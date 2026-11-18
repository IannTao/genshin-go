await app.setViewport(480, 760);
await app.press("扫码添加米哈游账号");
await app.waitFor("等待扫描：请使用米游社扫码", { timeout: 20000 });
console.log("请用米游社扫码并在手机上确认");
await app.waitFor("米游社授权已保存", { timeout: 120000 });
console.log("界面确认授权已保存");
