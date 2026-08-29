# 磁盘工程整理助手 / Disk Project Organizer

面向 Windows 的高性能原生桌面工具，用于发现大文件、精确识别重复文件，以及按用途整理 Git、CMake、Visual Studio、Python、Node.js、Rust、Go 等工程。

> [!IMPORTANT]
> 扫描操作只读；重复文件只会移入 Windows 回收站；工程整理会移动整个工程根目录。首次使用时请先扫描小范围目录，并在执行变更前核对选择项、目标路径和备份。

## 适合谁

- 开发盘被历史工程、构建产物和重复依赖占满，需要先看清再清理的人。
- 同时维护 Git、CMake、Visual Studio、Python、Node.js、Rust 或 Go 工程，希望按用途统一归档的人。
- 不愿把目录、文件名或内容上传云端，希望工具完全在本机运行的人。

本项目把“让工具替你决定”改成“让工具把证据和计划摆出来”：先扫描、再预览，只有明确选择和二次确认后才执行变更。核心安全行为有自动化测试，所有变更操作都有本地审计记录。

## 功能概览

| 功能 | 处理方式 | 是否修改文件 |
|---|---|---|
| 大文件扫描 | 并行遍历，按文件大小降序展示 | 否 |
| 精确查重 | 文件大小 → 首尾 BLAKE3 → 完整 SHA-256 | 扫描不修改；确认后移入回收站 |
| 工程识别 | 工程标记、语言、说明、Git/worktree 状态 | 否 |
| 工程整理 | 生成预览计划，确认后整体移动工程根目录 | 是 |
| 审计 | 记录回收与移动操作的结果、时间和路径 | 追加本机 JSONL 日志 |

## 运行时架构

[![磁盘工程整理助手运行时架构](docs/architecture/disk-project-organizer-architecture.svg)](docs/architecture/disk-project-organizer-architecture.png?raw=1)

架构图由仓库级 Archify 技能生成，并通过 showcase 校验与多分辨率浅色/深色视觉检查。README 使用可无损缩放的 SVG；点击图片会打开高分辨率 PNG，不再跳到 HTML 源文件。

- [打开 SVG 矢量图](docs/architecture/disk-project-organizer-architecture.svg?raw=1)
- [打开高分辨率 PNG](docs/architecture/disk-project-organizer-architecture.png?raw=1)
- [架构图 JSON 源](docs/architecture/disk-project-organizer.architecture.json)
- [文字版架构说明](docs/architecture.md)

交互式 HTML 不作为 GitHub 阅读入口；如需节点聚焦、路径追踪和主题切换，请下载 [`disk-project-organizer.html`](docs/architecture/disk-project-organizer.html) 后在本地浏览器打开。

核心运行链路如下：

```text
Windows 用户
  → egui 桌面界面
  → 后台任务控制器（线程 / 进度 / 取消）
  → dua-core 并行文件遍历
  ├─ 大文件扫描
  ├─ 三级精确查重
  └─ 工程识别与 Git 检查
       → 安全计划与用户确认
       → 回收重复文件 / 整体移动工程根
       → Git 一致性校验与 JSONL 审计
```

## 快速开始

### 环境要求

- Windows 桌面环境；回收站和字体加载使用 Windows 能力。
- Rust 1.95 或更高版本，使用 MSVC 工具链。
- 整理 Git 工程时，确保 `git` 命令已加入 `PATH`。如果界面显示 Git 为“无/未初始化”，请不要移动需要保留 Git 状态的工程，先确认 Git 可用。

### 从源码运行

```powershell
git clone https://github.com/carpentry-liu/disk-project-organizer.git
cd disk-project-organizer
cargo run --release
```

也可以使用开发脚本：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-dev.ps1
```

### 构建 Windows 可执行文件

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-release.ps1
```

脚本会依次执行格式检查、Clippy、测试和 Release 构建，成功后生成：

```text
dist\disk-project-organizer.exe
```

## 操作手册

### 1. 设置扫描范围

1. 启动程序后，在窗口顶部设置一个或多个扫描目录。
2. 点击“选择文件夹…”从文件选择器添加目录；也可以点击“添加 `C:\`”“添加 `D:\`”“添加 `E:\`”。快捷按钮只在对应盘符存在时显示。
3. 如需添加其他路径，在输入框填写绝对路径，再点击“添加路径”。只有当前存在的目录会被加入。
4. 点击路径右侧的“×”移除单个目录；点击“清空”移除全部目录。
5. 扫描过程中可点击底部“取消”。任务结束前，新的扫描和变更操作会保持禁用。

程序默认跳过系统目录、依赖目录、虚拟环境、缓存、构建产物和重解析点。无法读取的目录或文件不会进入结果，请结合底部状态和“日志与安全”页确认异常。

### 2. 查找大文件

1. 打开“大文件”页。
2. 设置“最小大小（GB）”；默认值为 `1.0 GB`。
3. 点击“开始扫描”。底部会显示当前阶段、消息和可用的进度百分比。
4. 扫描完成后查看“大小”“占用”“修改时间”“路径”。点击路径可打开文件所在目录。
5. 点击“导出 CSV”保存完整结果，默认文件名为 `large-files.csv`。

大文件列表按逻辑大小从大到小排序。“大小”是文件长度，“占用”是文件系统实际分配空间，两者可能因稀疏文件或簇大小而不同。

### 3. 精确查找并回收重复文件

1. 打开“重复文件”页。
2. 设置“最小大小（MB）”；默认值为 `10 MB`。提高阈值可减少磁盘读取和哈希时间。
3. 点击“精确查重”。程序只把完整 SHA-256 相同的文件放入同一重复组。
4. 展开重复组，核对文件大小、SHA-256 和每个路径。
5. 逐个勾选需要回收的副本，或点击“每组保留第一份”自动选择每组除第一条路径外的文件。
6. 点击“选中项移入回收站”，在二次确认窗口核对数量后点击“确认”。
7. 如需留档，点击“导出 CSV”，默认文件名为 `duplicates.csv`。

> [!WARNING]
> “每组保留第一份”只按排序后的路径保留第一项，不判断哪个文件更新、更重要或正在被其他工程引用。执行前必须逐组复核。程序会阻止回收某一重复组的全部副本。

重复检测分三阶段：

```text
相同文件大小
  → 读取首尾各最多 64 KiB，计算 BLAKE3 快速指纹
  → 对剩余候选读取完整内容，计算 SHA-256
  → 只保留完全一致的重复组
```

### 4. 识别并整理工程

1. 先按“设置扫描范围”添加可能包含工程的目录。
2. 打开“工程整理”页，在“工程库”中输入或选择目标根目录。
3. 点击“识别工程”。程序会根据工程标记、语言、README/入口信息和 Git 状态识别工程根。
4. 扫描完成后自动生成移动计划；如果后来修改了工程库路径，点击“重建计划”。
5. 逐行检查“新名称”“分类”“语言”“Git”“安全”“源路径”。点击“新名称”可编辑名称和分类，再点击“应用到计划”。
6. 计划生成后，所有标记为“安全”的项目会被默认选中。请先点击“清除选择”，再按需勾选；或在复核全部计划后使用“选择全部安全项目”。
7. 点击“导出计划”将完整计划保存为 `project-plan.csv`，建议在实际移动前留档。
8. 默认不要勾选“允许跨盘移动”。只有在确认目标盘空间充足、已有备份，并能接受复制耗时后再启用。
9. 点击“整理选中项目”，在确认窗口再次核对项目数量，然后点击“确认执行”。

默认分类如下：

| 分类 | 典型用途 |
|---|---|
| `00_Worktrees` | Git worktree、子模块或需要单独处理的关联工作区 |
| `01_仿真建模` | 仿真、建模、数字孪生相关工程 |
| `02_机器人` | 机器人、ROS、运动控制相关工程 |
| `03_AI工具` | AI、机器学习、智能工具 |
| `04_工业接口` | 工业协议、设备接口、PLC/OPC 等 |
| `05_Cpp_Qt` | C/C++、Qt、CMake、Visual Studio |
| `06_Python` | Python 工程 |
| `07_Web_Node` | Web、Node.js、前端工程 |
| `08_多语言` | 多语言混合工程 |
| `09_学习测试` | 学习、示例、实验和测试工程 |
| `10_SDK依赖` | SDK、第三方依赖或工具链 |
| `11_历史构建` | 历史构建与归档候选 |
| `90_待确认` | 信息不足，需要人工分类 |

工程整理的安全规则：

- 始终移动整个工程根目录，`.git` 会随工程一起移动，不会逐文件合并两个工程。
- `.git` 为文件的链接式 worktree/子模块，以及关联多个 worktree 的仓库，默认标记为不安全且不能勾选。
- 目标目录已存在时拒绝移动。
- 同盘移动使用目录重命名；跨盘移动使用复制后删除源目录，因此默认关闭。
- 如果扫描时记录了 Git 状态，移动后会核对 `HEAD`、当前分支和 `origin` 远程地址；失败时尝试回滚，并把结果写入审计日志。
- 移动前请关闭 IDE、终端、文件管理器预览和可能占用工程文件的进程。

### 5. 查看日志与安全状态

打开“日志与安全”页可以查看：

- 本次运行的扫描、导出、回收和移动消息；
- 当前审计日志路径；
- “打开日志目录”按钮。

持久化审计日志默认位于：

```text
%LOCALAPPDATA%\DiskProjectOrganizer\operations.jsonl
```

每行是一条 JSON 记录，包含 `success`、`action`、`source`、`destination`、`message` 和 `time`。只有会修改文件的回收与移动操作写入持久化审计；扫描结果需通过 CSV 单独导出。

## 安全模型

| 操作 | 默认保护 |
|---|---|
| 大文件扫描 | 只读 |
| 重复文件扫描 | 只读，只有 SHA-256 完全一致才成组 |
| 重复文件回收 | 显式勾选、二次确认、移入回收站、每组至少保留一份 |
| 工程扫描 | 只读 |
| 工程整理 | 先生成计划，仅允许勾选安全项目，二次确认后执行 |
| 跨盘移动 | 默认关闭 |
| Git worktree | 默认拒绝 |
| 已存在目标 | 拒绝覆盖 |
| Git 校验失败 | 尝试回滚并记录审计 |

更完整的说明见 [安全模型](docs/safety.md) 和 [安全报告策略](SECURITY.md)。

## 结果与文件位置

| 内容 | 位置或默认文件名 |
|---|---|
| 大文件结果 | 用户选择的 `large-files.csv` |
| 重复文件结果 | 用户选择的 `duplicates.csv` |
| 工程移动计划 | 用户选择的 `project-plan.csv` |
| 操作审计 | `%LOCALAPPDATA%\DiskProjectOrganizer\operations.jsonl` |
| Release 可执行文件 | `dist\disk-project-organizer.exe` |

为保持界面响应，结果表格最多显示 2,000 行；CSV 导出仍包含当前扫描产生的完整结果。

## 常见问题

| 现象 | 处理方法 |
|---|---|
| 扫描按钮不可用 | 先添加至少一个存在的扫描目录，并等待当前任务结束 |
| 工程计划为空 | 先设置“工程库”，完成工程识别后点击“重建计划” |
| 项目不能勾选 | 查看“安全”列；worktree、多工作树等风险项目会被禁用 |
| Git 显示“无/未初始化” | 确认项目确实是 Git 仓库，并检查 `git --version` 是否可在当前终端运行 |
| 跨盘移动被拒绝 | 保持默认行为，改用同盘工程库；确需跨盘时先备份并显式勾选允许项 |
| 目标目录已存在 | 修改计划中的名称/分类，或人工处理目标目录后重建计划 |
| 扫描耗时较长 | 缩小扫描范围、提高大小阈值，或等待当前 SHA-256 大文件读取完成后取消 |
| 变更结果不确定 | 立即停止后续操作，查看“日志与安全”页和 `operations.jsonl` |

## 开发与验证

常规质量检查：

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

本机自检：

```powershell
cargo run --release -- --self-test .\self-test-output.json
```

本机性能基准：

```powershell
cargo run --release -- --benchmark .\benchmark-output.json
```

基准会在临时目录生成约 5,000 个小文件，测试大文件遍历、三级查重和工程识别，完成后删除测试数据。性能设计和基线见 [性能说明](docs/performance.md)。

### 更新架构图

Archify 仅安装在本仓库的 `.agents/skills/archify`，不会影响其他项目。更新代码结构后可在仓库根目录执行：

```powershell
node .agents\skills\archify\bin\archify.mjs validate architecture docs\architecture\disk-project-organizer.architecture.json --quality showcase --json
node .agents\skills\archify\bin\archify.mjs deliver architecture docs\architecture\disk-project-organizer.architecture.json docs\architecture\disk-project-organizer.html --quality showcase --repo-root . --json
node scripts\export-architecture-assets.mjs
node .agents\skills\archify\bin\archify.mjs visual-check docs\architecture\disk-project-organizer.html --json
```

导出脚本会从自包含 HTML 生成 GitHub 使用的双主题 SVG 和高分辨率 PNG。交付前必须确认 9/9 showcase 校验、0 错误、0 警告、导出 receipt 为 canonical，并人工检查 visual-check 生成的浅色/深色截图与 PNG。

## 为什么使用 Rust

- `dua-core` 使用工作窃取线程池和 Windows 原生目录枚举。
- Rayon 并行处理重复候选的快速指纹和完整哈希。
- 原生单文件 `.exe`，运行时不要求安装 Python。
- 强类型计划与显式安全校验减少误删、误搬风险。
- `unsafe_code = "forbid"`，并在 Release 配置中启用 Thin LTO。

## 贡献

提交代码前请阅读 [贡献指南](CONTRIBUTING.md) 和 [AI 辅助开发与 Vibe Coding 约定](docs/ai-assisted-development.md)，并确保格式检查、Clippy 和测试全部通过。项目欢迎 AI 辅助贡献，但合入依据始终是清晰范围、可审查 diff、真实验证证据和人工复核，而不是使用了哪一种工具。

## 许可证

本项目使用 [MIT License](LICENSE)。
