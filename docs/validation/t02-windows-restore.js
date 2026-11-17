await app.setViewport(480, 760);
const saved = app.texts().find((value) => value.startsWith("米哈游账号："));
if (!saved) throw new Error("重开后未恢复米哈游账号");
console.log("重开恢复账号通过");
