# dsh-gui

DeepSeek Harness 的 Tauri 桌面壳：调用**全局安装**的 `dsh` 命令拉起 web
服务，再把页面嵌入带精简自定义标题栏的桌面窗口。在 Windows 上既可运行本机
`dsh`，也可从标题栏选择一个 WSL 发行版并运行其中安装的 Linux 版 `dsh`。
项目本身**不依赖 dsh 的任何 npm 包**——不 import、不 require，只以子进程
方式调用命令行。

## 工作原理

1. 启动时自动定位并启动 Windows 全局 `dsh`（`where.exe dsh`，优先 `.cmd`
   shim，兜底 `%APPDATA%\npm\dsh.cmd`）。标题栏同时列出 `wsl.exe -l -q`
   返回的发行版；WSL 实例由用户明确选择发行版后再启动。
2. Windows 以 `dsh web --port 0` 启动。WSL 实例先在 Windows 侧预留独立
   空闲端口，再通过所选发行版的登录 shell 启动 dsh，以兼容 NVM 等只在登录
   环境配置 Node PATH 的安装方式。程序只接受发行版内的 Linux 原生 dsh 路径，
   不会误用 WSL PATH 中 `/mnt/c/...` 指向的 Windows npm shim。两个实例拥有
   独立的进程、端口和日志。
3. 读取各子进程 stdout，等到就绪行 `dsh web: http://127.0.0.1:<port>`
   （dsh web 打印该行即表示服务已绑定端口），把地址交给窗口内的 Web 区域
   加载。Windows 与 WSL 使用两个常驻 iframe，标题栏切换时不会卸载另一边
   的页面。WSL 运行期间改选发行版不会立即中断当前实例，管理菜单会提供明确的
   `Switch to <发行版>` 操作。
4. 期间子进程的 stdout/stderr 实时转发到加载页显示（首次启动如要安装
   profile 依赖会花较长时间，日志可见）。
5. 可以在标题栏分别启动、停止或重启 Windows / WSL 实例。关闭窗口时会清理
   两棵进程树（Windows：`taskkill /PID <pid> /T /F`），不留孤儿进程。
6. Tauri 官方 window-state 插件在退出时保存窗口位置、大小和最大化状态，
   下次启动自动恢复。

## 结构

```
ui/                双实例标题栏、加载页和 dsh Web 容器（无构建步骤、无 node 依赖）
src-tauri/         Tauri v2 Rust 工程
  src/lib.rs       双实例逻辑：定位 dsh、WSL 发行版、spawn、就绪检测、退出清理
  tauri.conf.json  窗口配置；withGlobalTauri 开启，页面直接用 __TAURI__
scripts/gen-icons.ps1  图标生成脚本（Windows，System.Drawing）
```

## 构建

前置条件：Rust 工具链；Windows 全局安装 `npm i -g @deepseek-ai/dsh`；
Windows 需 WebView2 运行时（Win10/11 一般自带）。WSL 功能是可选的；需要在
希望使用的每个发行版**内部**另行安装 dsh，并确保登录 shell 能找到它。

```sh
cd src-tauri
cargo build --release          # 产物 target/release/dsh-gui.exe
cargo tauri build              # 可选：打安装包（需 cargo install tauri-cli）
```

开发调试用 `cargo build`（debug 版带控制台，`println!`/日志可见）。

## 说明

- 窗口关闭即结束：Windows 与 WSL dsh 服务进程树随窗口一起退出。
- Windows 与所选 WSL 发行版会拉起**独立**的 `dsh web`。两边可以同时运行，
  在标题栏即时切换，也不会占用/接管已经打开的 web 实例。
- 若未安装 dsh，窗口会显示错误提示而非静默失败。
- Windows 下用于定位、启动和清理 dsh 的 `where.exe`、`cmd.exe`、
  `taskkill.exe` 均以无控制台窗口模式运行，不会闪出黑框。
