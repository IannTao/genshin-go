await app.setViewport(480, 760);
const saved = app.texts().find((value) => value.startsWith("米哈游账号："));
if (!saved) throw new Error("重开后未恢复米哈游账号");
const verified = app.texts().find((value) => /^已验证游戏 UID：[1-9]\d{8,9}$/.test(value));
if (!verified) throw new Error("重开后未恢复经验证的游戏 UID");
console.log("重开恢复账号及游戏 UID 通过");
