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
