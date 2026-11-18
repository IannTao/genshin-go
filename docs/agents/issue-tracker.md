# Issue tracker: GitHub

仓库：IannTao/genshin-go。
使用 gh CLI；命令显式指定 --repo IannTao/genshin-go，
不依赖当前目录的 Git remote。

- 发布规格或任务：创建 GitHub issue。
- 创建前检查已有 issue，避免重复发布。
- 多行正文写入 UTF-8 文件，通过 --body-file 提交。
- 读取任务时包含正文、标签和评论。
- 标签使用 triage-labels.md 中的映射。
- 子任务优先使用 GitHub sub-issues；不可用时使用父子链接。
- 阻塞关系优先使用原生依赖；不可用时记录 Blocked by。
- wayfinder 使用主 issue 保存探索地图，子 issue 保存任务；
  领取时分配执行者，完成时记录结论、关闭任务并更新主 issue。

PRs as a request surface: no.

## 一期任务的批量维护

在仓库根目录用 PowerShell 7.2 或更新版本运行。修改标题、正文或前置任务时，更新 `docs/specs/launcher-tickets/` 中的 `publication-plan.json` 和对应票稿。取消任务时保留 T 编号，设置 `status` 为 `cancelled` 并移除相关依赖。正常完成任务按下节回写 GitHub，当前核验会识别 `completed` 状态。

- `./docs/specs/publish-launcher-tickets.ps1` 预览差异。确认后加 `-Apply` 写入 GitHub。创建中断后用同一命令重跑，脚本按 T 编号找回已创建的票，并在整批成功后原子保存完整映射。
- `./docs/specs/verify-launcher-tickets.ps1` 只读检查当前计划、子任务关系、正文和原生依赖。已勾选的验收项保留进度，取消票保留原正文。首次发布快照独立保存。

## Issue 完成回写

1. 在对应的 GitHub Issue 下发表评论，简述交付结果、实际执行的验证及其结果。有验收清单时，勾选已经验证通过的条目。
2. 全部验收条件满足后，以 `completed` 原因关闭该 Issue。仍有未满足的条件时，保持打开，并在评论中说明剩余事项及原因。
3. 回读该 Issue，确认评论和状态已同步，在交付说明中附上 Issue 链接。同步失败时说明具体错误和待回写内容。
