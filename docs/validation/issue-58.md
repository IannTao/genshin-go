# #58 Windows 键盘安全移除授权与清理指定 UID 验收

验证「米游社 → 管理授权 → 移除账号」与「角色资料 → 资料管理 → 删除历史记录」两个独立确认框的完整目标标识、影响范围、键盘阅读顺序、框内焦点限制、Enter 默认取消、Esc 返回及焦点归还。

## 重跑步骤

在仓库根目录执行 Windows 隔离夹具的原生窗口键盘测试：

```powershell
Push-Location tests/.fixture
nsh test .windows/authorization.test.ts --target windows
nsh test .windows/history.test.ts --target windows
Pop-Location
```

如需在真实启动器中手工复验，请使用独立数据目录启动：

```powershell
$env:GENSHIN_GO_DATA_DIR = Join-Path ([IO.Path]::GetTempPath()) ('genshin-go-issue58-' + [guid]::NewGuid())
cargo run --locked
```

1. 进入「米游社」，授权账号后展开「管理授权」，按 `Tab` 依次经「检查授权：`<账号>`」移至「移除账号：`<账号>`」，按 `Enter` 打开确认框。核对标题与危险按钮显示完整米哈游账号、说明自动签到暂停与各 UID 资料保留。
2. 不按 `Tab` 直接按 `Enter`，核对默认触发「取消」且账号仍在；随即再按 `Enter`，核对焦点已归还至触发按钮并重新打开确认框。
3. 在确认框内连续按两次 `Tab`，核对焦点在「取消」与「确认移除账号：`<账号>`」间循环且不逸出至底层页面，按 `Enter` 取消并归还焦点。
4. 再次 `Enter` 打开确认框，按 `Tab` 移至危险按钮后按 `Esc`，核对安全返回、账号仍在且焦点归还。再次 `Enter` → `Tab` → `Enter` 完成移除，核对自动签到暂停且各 UID 资料继续可用。
5. 进入「角色资料」，在 `game-uid` 输入框填入 9 至 10 位 UID 后按 `Tab` → `Enter` 创建空间，再按 `Tab` → `Enter` 展开「资料管理」，按 `Tab` → `Enter` 打开删除历史确认框。核对标题与危险按钮显示完整目标 UID 及清除/保留范围，按上述步骤分别验证直接 `Enter` 默认取消、`Tab` 框内循环、`Esc` 返回、焦点归还与确认删除。

## 实机记录

2026-11-22，在 Windows 11 上以 NSH 0.4.0 原生 Windows 宿主（`--target windows`）执行两项实机键盘测试，均通过：

| 验证场景 | 实测结果 |
| --- | --- |
| 移除账号确认框（`.windows/authorization.test.ts`） | `Tab` 按阅读顺序到达「移除账号：10001」并由 `Enter` 打开；首次直接 `Enter` 默认取消并归还焦点（再次 `Enter` 重开）；框内两次 `Tab` 限制在两个按钮间并回到「取消」；`Tab` 移至危险按钮后按 `Esc` 取消并归还焦点；再次 `Enter` → `Tab` → `Enter` 成功移除授权并保留 UID 数据。 |
| 删除历史记录确认框（`.windows/history.test.ts`） | `Tab` 依次经 `game-uid` →「创建 UID 数据空间」→「资料管理」→「删除历史记录」并由 `Enter` 打开；首次直接 `Enter` 默认取消并归还焦点；框内两次 `Tab` 限制并回到「取消」；`Tab` 移至危险按钮后按 `Esc` 取消并归还焦点；再次 `Enter` → `Tab` → `Enter` 成功清除 UID `1987654321` 的本机历史记录，随后焦点归还至「资料管理」。 |
| 近七个北京日期与未检查日期 | 隔离夹具及 Rust 单元测试验证当日结果与前六个北京日期完整展示，未检查日期明确标为「未检查」，跳过日期保留既有检查结果，失败重试、授权过期重新扫码与奖励待确认检查领取均正常。 |
| 历史清理范围与文件保留 | 根组件集成测试（`App.test.tsx`）验证仅清除目标 UID 业务记录，保留其 UID 空间、其他 UID 记录，以及磁盘上的 UIAF 导出文件与整体备份文件。 |
