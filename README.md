# dsh-gui

[![Build](https://github.com/gmryuuko/dsh-gui/actions/workflows/build.yml/badge.svg)](https://github.com/gmryuuko/dsh-gui/actions/workflows/build.yml)

一个面向 Windows 的轻量级 dsh 桌面客户端。它会启动系统中已安装的 `dsh web`，并在
原生窗口中显示界面。

## 功能

- 支持 Windows 和 WSL 中的 dsh，可同时运行并快速切换。
- 在标题栏查看实例状态、版本并进行启动、停止和重启。
- 自动发现 WSL 发行版。
- 保存窗口位置、大小和最大化状态。
- 关闭应用时自动清理所启动的 dsh 进程。

## 使用

需要 Windows 10/11，并提前全局安装 dsh：

```powershell
npm install --global @deepseek-ai/dsh
dsh --version
```

从 [Releases](https://github.com/gmryuuko/dsh-gui/releases/latest) 下载最新版
`dsh-gui-windows-x64.exe`，直接运行即可。

如需使用 WSL，请在对应发行版中也安装 dsh，然后从标题栏选择该发行版。

> dsh-gui 不包含 dsh。Release 目前未签名，Windows 首次运行时可能显示安全提示。

## 从源码构建

需要 Rust stable、MSVC C++ Build Tools 和 WebView2：

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo build --release --locked --manifest-path src-tauri/Cargo.toml
```

构建产物位于 `src-tauri/target/release/dsh-gui.exe`。
