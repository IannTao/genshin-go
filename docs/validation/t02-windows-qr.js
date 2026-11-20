await app.setViewport(480, 760);
await app.press("米游社");
await app.press("扫码添加米哈游账号");
await app.waitFor("等待扫描：请使用米游社扫码", { timeout: 20000 });
await app.screenshot("target/t02-windows-qr.png");
await app.press("取消扫码");
await app.waitFor("已取消扫码");
