# dsh-gui

[![Build](https://github.com/gmryuuko/dsh-gui/actions/workflows/build.yml/badge.svg)](https://github.com/gmryuuko/dsh-gui/actions/workflows/build.yml)

一个面向 Windows 的轻量级 [Tauri](https://tauri.app/) 桌面客户端，用原生窗口运行
DeepSeek Harness 的 `dsh web`。它可以同时管理 Windows 与 WSL 中的 dsh 实例，并在
两个 Web 界面之间即时切换。

> dsh-gui 只负责桌面窗口和进程管理，不包含 dsh，也不依赖 dsh 的 npm 包。
> 使用前需要在 Windows 或目标 WSL 发行版中单独安装 `dsh` 命令。

## 功能

- 启动时自动发现并运行 Windows 全局安装的 `dsh`。
- 枚举 WSL 发行版，并在自绘菜单中显示各发行版的 dsh 安装状态与版本。
- Windows 与 WSL 实例使用独立进程、动态端口、日志和 Web 视图，互不干扰。
- 标题栏持续显示当前实例的运行状态和 dsh 版本，可启动、停止、重启或切换实例；
  切换时不会卸载另一侧页面。
- 实时显示 dsh 的启动日志和错误信息。
- 关闭窗口时清理所有子进程，避免留下后台服务。
- 自动保存并恢复窗口大小、位置和最大化状态。
- 前端是随程序打包的静态 HTML，无 Node.js 构建步骤和运行时依赖。

## 运行要求

| 项目 | 要求 |
| --- | --- |
| 操作系统 | Windows 10/11 x64 |
| WebView | Microsoft Edge WebView2（Windows 10/11 通常已预装） |
| Windows dsh | 全局安装，且 `where.exe dsh` 可以找到 |
| WSL dsh | 可选；需要在每个要使用的发行版中分别安装 |

在 Windows 中安装并确认 dsh：

```powershell
npm install --global @deepseek-ai/dsh
dsh --version
```

如果需要使用 WSL 实例，请进入对应发行版后再次安装：

```sh
npm install --global @deepseek-ai/dsh
dsh --version
```

dsh-gui 会通过发行版的登录 shell 查找命令，因此也兼容由 NVM 管理的 Node.js。
为避免误启动 Windows npm shim，WSL 实例只接受发行版内的 Linux 原生 dsh 路径。

## 获取与使用

1. 打开仓库的 [Actions](https://github.com/gmryuuko/dsh-gui/actions/workflows/build.yml) 页面。
2. 进入最近一次成功的 **Build** 运行，下载 `dsh-gui-windows-x64` 产物。
3. 解压并运行 `dsh-gui.exe`。
4. Windows 实例会自动启动；需要 WSL 时，在标题栏选择发行版并启动实例。

当前 CI 产物是未签名的便携版可执行文件，不是安装包。Windows SmartScreen 可能会在
首次运行时显示提示。

## 从源码构建

准备 Rust stable 工具链、MSVC C++ Build Tools 和 WebView2 开发环境。运行 dsh-gui
时仍需安装 dsh，但编译本身不依赖 dsh。

```powershell
git clone git@github.com:gmryuuko/dsh-gui.git
cd dsh-gui

# 开发运行
cargo run --manifest-path src-tauri/Cargo.toml

# 测试
cargo test --locked --manifest-path src-tauri/Cargo.toml

# Release 构建
cargo build --release --locked --manifest-path src-tauri/Cargo.toml
```

构建产物位于 `src-tauri/target/release/dsh-gui.exe`。

## 工作原理

1. 程序查找 Windows 全局 dsh，并以 `dsh web --port 0` 启动独立服务。
2. 用户选择 WSL 发行版后，程序在 Windows 侧分配空闲端口，再通过该发行版的登录
   shell 启动 dsh。
3. 后端读取 stdout 中的 `dsh web: http://127.0.0.1:<port>` 就绪信息，并把地址交给
   对应的内嵌 WebView。
4. 两个实例的生命周期和日志分别管理；退出桌面应用时统一清理进程树。

每次启动都会创建新的 dsh web 实例，不会接管已经运行的默认端口服务。

## 项目结构

```text
ui/                         静态前端、标题栏、实例切换和 Web 容器
src-tauri/
  src/lib.rs                dsh 发现、Windows/WSL 进程与 Tauri 命令
  src/main.rs               桌面程序入口
  capabilities/default.json Tauri 权限配置
  tauri.conf.json           窗口和应用配置
scripts/gen-icons.ps1       Windows 图标生成脚本
.github/workflows/build.yml Windows 测试、Release 构建和产物上传
```

## 常见问题

### 提示找不到 dsh

先在与 dsh-gui 相同的环境中运行 `dsh --version`。Windows 实例需要 `where.exe dsh`
能够返回全局命令；WSL 实例需要发行版的登录 shell 能够找到 Linux 原生 dsh。

### WSL 列表中没有目标发行版

运行 `wsl.exe -l -q` 确认发行版已经安装并完成首次初始化，然后重新打开 dsh-gui。

### 首次启动时间较长

dsh 首次启动可能需要安装或初始化 profile 依赖。启动过程会实时显示在窗口日志中，
完成后页面会自动载入。
