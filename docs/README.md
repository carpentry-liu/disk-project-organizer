# 项目文档索引

文档既服务于使用者和评审者，也是 AI 辅助开发时可核验的项目上下文。代码与文档不一致时，先确认哪个已经过时，再同步修正。

## 使用与信任

- [项目 README 与操作手册](../README.md)
- [安全模型](safety.md)
- [安全问题报告](../SECURITY.md)
- [性能设计与基线](performance.md)

## 架构

- [文字版架构说明](architecture.md)
- [高清 SVG](architecture/disk-project-organizer-architecture.svg)
- [高清 PNG](architecture/disk-project-organizer-architecture.png)
- [Archify JSON 事实源](architecture/disk-project-organizer.architecture.json)
- [交互式 HTML（下载后本地打开）](architecture/disk-project-organizer.html)

## 开发协作

- [AI 辅助开发与 Vibe Coding 约定](ai-assisted-development.md)
- [贡献指南](../CONTRIBUTING.md)
- [轻量变更提案模板](templates/change-proposal.md)

非平凡功能、安全策略变化、跨模块重构或不可逆决策，应复制变更提案模板到 `docs/changes/YYYY-MM-DD-<slug>.md`。小型文档修正和局部缺陷无需为了流程制造空文档，但仍要留下真实验证证据。
