# 贡献指南

感谢你帮助改进磁盘工程整理助手。这个项目会接触真实文件，因此我们优先接受范围清楚、有验证证据、不会削弱默认保护的改动。

## 开始之前

1. 阅读 [`AGENTS.md`](AGENTS.md)、[`docs/ai-assisted-development.md`](docs/ai-assisted-development.md) 和 [`docs/safety.md`](docs/safety.md)。
2. 安装 Rust 1.95 或更高版本、MSVC 工具链和 Git。
3. 从 Issue 描述用户问题、复现步骤或验收标准；安全漏洞请按 [`SECURITY.md`](SECURITY.md) 私下报告。
4. 非平凡功能、跨模块重构或安全行为变化，先复制 [`docs/templates/change-proposal.md`](docs/templates/change-proposal.md) 到 `docs/changes/`。

## 开发原则

- 先定位根因，再做最小且完整的修复。
- 扫描保持只读；文件变更保持预览、确认、保守拒绝和审计。
- 不把无关重构、依赖升级和功能修改塞进同一个 PR。
- 新依赖需说明现有依赖为何不够、许可证和发布体积影响。
- AI 辅助产出与人工代码采用相同标准，不以工具名称代替评审与测试。

## 本地验证

提交 Rust 改动前执行：

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

涉及完整桌面行为时，再执行：

```powershell
cargo run --release -- --self-test .\self-test-output.json
```

修改回收或移动逻辑时，必须覆盖正常、拒绝、失败、取消、回滚和审计场景。未运行的平台或检查要在 PR 中写明原因。

## 分支、Commit 与 PR

- 建议分支名：`feat/<topic>`、`fix/<topic>`、`docs/<topic>`。
- 使用标准 Conventional Commits：`<type>(<scope>): <summary>`。
- 标题使用祈使式、说明具体结果，建议不超过 72 个字符。
- 每个 commit 只表达一个意图；不要使用“更新”“修改一下”“若干优化”等空描述。

示例：

```text
feat(scanner): add cancellable duplicate hashing
fix(operations): preserve audit record after move rollback
docs: publish high-resolution architecture assets
```

PR 应填写仓库模板，至少包含用户价值、范围、风险、实际验证命令与结果、截图（如适用）以及 AI 参与和人工复核范围。

## 文档与架构图

- 用户可见行为变化必须同步 README 操作手册。
- 安全边界变化同步 `docs/safety.md`；性能取舍同步 `docs/performance.md`；模块边界变化同步架构说明和 Archify 资产。
- README 架构图使用 SVG，PNG 用于高清下载，HTML 只作为下载后本地打开的交互产物。
- 架构图更新命令见 [`AGENTS.md`](AGENTS.md) 的“架构图规则”。

## 评审更容易通过的做法

- 先用一句话说明使用者得到的价值，再说明实现。
- 把非目标写出来，降低评审者对范围失控的担忧。
- 给可复制的命令和真实结果，不只列待办复选框。
- UI 变化附截图；风险操作展示确认、拒绝和回滚路径。
- 保持 PR 小，让维护者可以逐步验证和回退。

