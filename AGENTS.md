# 磁盘工程整理助手：AI 协作入口

## 适用范围与优先级

- 本文件适用于整个仓库；如果未来子目录出现更近层级的 `AGENTS.md`，目标文件以最近层级规则为准。
- 用户当前要求优先，其次是最近层级规则，最后是本文件。规则冲突、需求不清或证据不足时必须显式说明。
- 开始工作前先检查 Git 状态、相关文档、现有实现和依赖；保留用户的无关改动。

## 项目定位

本项目是 Windows 本地优先的 Rust 桌面工具，用于扫描大文件、精确查重和整理开发工程。扫描可以是只读的，涉及回收或移动的动作必须坚持“先预览、再确认、可审计”。

## 不可破坏的安全边界

1. 扫描默认只读，不上传文件名、路径、哈希或内容。
2. 重复文件只有完整 SHA-256 一致才可归为同组；不得允许回收一组中的全部副本。
3. 删除必须进入 Windows 回收站，不新增不可恢复的永久删除路径。
4. 工程整理必须移动完整工程根，不做逐文件合并，不覆盖已存在的目标目录。
5. 跨盘移动、Git worktree、多工作树仓库继续采用保守策略；任何放宽都属于高风险改动。
6. 变更文件系统前必须展示计划并获得用户确认；执行结果必须写入审计日志。
7. 不读取、提交或展示凭据、令牌、私人路径内容和其他敏感信息。

修改以上边界时，必须先写变更提案，列出风险、回滚方式和对应测试，再进入实现。

## 仓库地图

| 路径 | 职责 |
|---|---|
| `src/app.rs` | egui 界面、交互状态和任务调度 |
| `src/scanner.rs` | 大文件与重复文件扫描 |
| `src/projects.rs` | 工程识别、分类、计划和 Git 检查 |
| `src/operations.rs` | 回收、移动、回滚与审计 |
| `src/model.rs` | 跨模块数据模型 |
| `src/util.rs` | 路径、大小等共享工具 |
| `tests/` | 核心行为与安全回归测试 |
| `docs/` | 架构、安全、性能和 AI 协作文档 |
| `scripts/` | 开发、发布和架构图导出脚本 |

## 工作方式

1. 用“目标、范围、非目标、约束、验收条件”复述任务；能从仓库确认的事实不要向用户追问。
2. 小型文档修正和局部缺陷可直接实施。新增功能、跨模块重构、安全策略变化或不可逆决策，先复制 `docs/templates/change-proposal.md` 到 `docs/changes/` 并完成评审信息。
3. 调研、方案、实施、验证分开记录。结论必须来自代码、仓库文档、测试或明确的外部来源，不把模型记忆当证据。
4. 优先完成最小的端到端改动；不做没有当前使用者的抽象、配置或兼容层，不顺手扩大范围。
5. AI 可以提出和实现方案，但不能替人确认高风险文件操作、批准自己的变更或把未运行的检查写成通过。
6. 代码、用户手册、安全说明、架构图和变更模板保持同步。

完整流程见 `docs/ai-assisted-development.md`。

## 架构图规则

- Archify 只安装在 `.agents/skills/archify`，不得改为用户级或全局安装。
- 架构事实源是 `docs/architecture/disk-project-organizer.architecture.json`；HTML、SVG 和 PNG 都是生成物。
- README 使用 SVG 作为清晰的 GitHub 入口，PNG 作为高分辨率备用；不要把 README 图片链接到 HTML 源文件。
- 更新架构后依次执行：

```powershell
node .agents\skills\archify\bin\archify.mjs validate architecture docs\architecture\disk-project-organizer.architecture.json --quality showcase --json
node .agents\skills\archify\bin\archify.mjs deliver architecture docs\architecture\disk-project-organizer.architecture.json docs\architecture\disk-project-organizer.html --quality showcase --repo-root . --json
node scripts\export-architecture-assets.mjs
node .agents\skills\archify\bin\archify.mjs visual-check docs\architecture\disk-project-organizer.html --json
```

交付前确认 showcase 校验通过、导出 receipt 为 canonical，并人工检查浅色/深色截图和高清 PNG。

## 验证与完成定义

Rust 最低版本为 1.95。代码改动至少执行：

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

- 涉及真实文件变更逻辑时，补充失败、取消、回滚和审计测试。
- 涉及 UI 时，记录人工操作路径或截图；涉及性能时，给出可复现基准数据。
- 文档改动检查本地链接、命令、版本号和图像资源。
- 最终回执逐条写出“执行的命令 + 实际结果”；未执行项必须说明原因。

## Git 约定

- 使用标准 Conventional Commits：`<type>(<scope>): <imperative summary>`；`scope` 可省略。
- 常用类型：`feat`、`fix`、`docs`、`refactor`、`test`、`perf`、`build`、`ci`、`chore`。
- 一个提交只表达一个意图，标题具体，不使用“更新一下”“若干修改”等空描述。
- 禁止跳过 hooks、强推共享分支或覆盖无关改动，除非用户明确授权。
- 推送、创建发布、修改仓库可见性等外部写操作必须获得用户明确授权。
