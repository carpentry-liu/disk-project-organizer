# 磁盘工程整理助手 / Disk Project Organizer

面向 Windows 的高性能原生桌面工具，用于：

1. 快速发现大文件；
2. 精确识别重复文件；
3. 识别 Git、CMake、Visual Studio、Python、Node、Rust、Go 工程；
4. 按用途汇总整理工程，并保证 `.git` 随工程根目录整体移动。

## 为什么使用 Rust

- `dua-core` 使用工作窃取线程池和 Windows 原生目录枚举；
- 重复检测分三阶段，避免对所有文件直接计算 SHA-256；
- Rayon 并行读取候选文件；
- 原生单文件 `.exe`，不要求安装 Python；
- 强类型计划与安全校验减少误删、误搬风险。

## 重复检测流程

```text
文件大小分组 → 首尾采样 BLAKE3 → 完整 SHA-256 → 确认完全重复
```

只有 SHA-256 相同的文件才进入重复组。删除操作会移入 Windows 回收站，而且每组至少保留一份。

## 工程整理流程

```text
发现工程根 → 读取 README/入口文件/Git → 推断用途与名称
        → 生成预览计划 → 用户确认 → 整体移动工程根 → Git 校验
```

工具不会把两个工程目录逐文件混合，也不会自动移动 Git worktree、SDK、虚拟环境、构建目录或系统软件目录。

## 开发运行

```powershell
cargo run --release
```

## 构建 Windows 可执行文件

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-release.ps1
```

输出：`dist\disk-project-organizer.exe`

## 自检

```powershell
cargo run --release -- --self-test .\self-test-output.json
```

## 本机性能基准

```powershell
cargo run --release -- --benchmark .\benchmark-output.json
```

基准会在临时目录生成约 5,000 个小文件，测试大文件遍历、三级查重和工程识别，完成后自动删除测试数据。

## 仓库

计划发布到：<https://github.com/carpentry-liu/disk-project-organizer>
