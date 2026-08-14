//! dsh-gui — a desktop shell for native Windows and WSL dsh web instances.
//!
//! The shell never links to dsh. It starts the globally installed command as
//! a child process, waits for its loopback URL, and embeds that URL. Native
//! Windows dsh starts automatically; a Linux-native dsh in a titlebar-selected
//! WSL distribution can be started independently.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager, RunEvent, State};

const MAX_KEPT_LOG_LINES: usize = 200;
const EV_INSTANCE: &str = "dsh-instance";
const URL_MARKER: &str = "http://127.0.0.1:";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InstanceKind {
    Windows,
    Wsl,
}

impl InstanceKind {
    const ALL: [Self; 2] = [Self::Windows, Self::Wsl];

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "windows" => Ok(Self::Windows),
            "wsl" => Ok(Self::Wsl),
            _ => Err(format!("未知 dsh 实例：{value}")),
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Wsl => "wsl",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::Wsl => "WSL",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum InstancePhase {
    Stopped,
    Starting,
    Ready,
    Error,
}

struct InstanceRuntime {
    child: Option<Child>,
    generation: u64,
    revision: u64,
    phase: InstancePhase,
    url: Option<String>,
    detail: Option<String>,
    version: Option<String>,
    error: Option<String>,
    log: VecDeque<String>,
}

impl InstanceRuntime {
    fn new() -> Self {
        Self {
            child: None,
            generation: 0,
            revision: 0,
            phase: InstancePhase::Stopped,
            url: None,
            detail: None,
            version: None,
            error: None,
            log: VecDeque::new(),
        }
    }

    fn snapshot(&self, kind: InstanceKind) -> InstanceSnapshot {
        InstanceSnapshot {
            kind: kind.id(),
            label: kind.label(),
            revision: self.revision,
            phase: self.phase,
            url: self.url.clone(),
            detail: self.detail.clone(),
            version: self.version.clone(),
            error: self.error.clone(),
            log: self.log.iter().cloned().collect(),
        }
    }
}

struct DshState {
    windows: Mutex<InstanceRuntime>,
    wsl: Mutex<InstanceRuntime>,
}

impl DshState {
    fn new() -> Self {
        Self {
            windows: Mutex::new(InstanceRuntime::new()),
            wsl: Mutex::new(InstanceRuntime::new()),
        }
    }

    fn runtime(&self, kind: InstanceKind) -> &Mutex<InstanceRuntime> {
        match kind {
            InstanceKind::Windows => &self.windows,
            InstanceKind::Wsl => &self.wsl,
        }
    }

    fn snapshot(&self, kind: InstanceKind) -> InstanceSnapshot {
        self.runtime(kind).lock().unwrap().snapshot(kind)
    }

    fn emit_snapshot(&self, handle: &tauri::AppHandle, kind: InstanceKind) {
        let _ = handle.emit(EV_INSTANCE, self.snapshot(kind));
    }

    fn push_log(
        &self,
        handle: &tauri::AppHandle,
        kind: InstanceKind,
        generation: u64,
        line: String,
    ) -> bool {
        {
            let mut runtime = self.runtime(kind).lock().unwrap();
            if runtime.generation != generation {
                return false;
            }
            runtime.log.push_back(line);
            while runtime.log.len() > MAX_KEPT_LOG_LINES {
                runtime.log.pop_front();
            }
            runtime.revision = runtime.revision.wrapping_add(1);
        }
        self.emit_snapshot(handle, kind);
        true
    }

    fn set_ready(
        &self,
        handle: &tauri::AppHandle,
        kind: InstanceKind,
        generation: u64,
        url: String,
    ) {
        {
            let mut runtime = self.runtime(kind).lock().unwrap();
            if runtime.generation != generation {
                return;
            }
            runtime.phase = InstancePhase::Ready;
            runtime.url = Some(url);
            runtime.error = None;
            runtime.revision = runtime.revision.wrapping_add(1);
        }
        self.emit_snapshot(handle, kind);
    }

    fn set_error(
        &self,
        handle: &tauri::AppHandle,
        kind: InstanceKind,
        generation: u64,
        message: String,
    ) {
        {
            let mut runtime = self.runtime(kind).lock().unwrap();
            if runtime.generation != generation {
                return;
            }
            runtime.phase = InstancePhase::Error;
            runtime.url = None;
            runtime.error = Some(message);
            runtime.revision = runtime.revision.wrapping_add(1);
        }
        self.emit_snapshot(handle, kind);
    }

    fn take_child(&self, kind: InstanceKind, generation: u64) -> Option<Child> {
        let mut runtime = self.runtime(kind).lock().unwrap();
        (runtime.generation == generation)
            .then(|| runtime.child.take())
            .flatten()
    }

    fn stop_all(&self) -> Vec<Child> {
        let mut children = Vec::new();
        for kind in InstanceKind::ALL {
            let mut runtime = self.runtime(kind).lock().unwrap();
            runtime.generation = runtime.generation.wrapping_add(1);
            runtime.revision = runtime.revision.wrapping_add(1);
            if let Some(child) = runtime.child.take() {
                children.push(child);
            }
            runtime.phase = InstancePhase::Stopped;
            runtime.url = None;
            runtime.detail = None;
            runtime.error = None;
        }
        children
    }
}

#[derive(Clone, Serialize)]
struct InstanceSnapshot {
    kind: &'static str,
    label: &'static str,
    revision: u64,
    phase: InstancePhase,
    url: Option<String>,
    detail: Option<String>,
    version: Option<String>,
    error: Option<String>,
    log: Vec<String>,
}

#[derive(Serialize)]
struct StatusSnapshot {
    instances: Vec<InstanceSnapshot>,
    wsl_distributions: Vec<WslDistributionInfo>,
}

#[derive(Serialize)]
struct WslDistributionInfo {
    name: String,
    version: Option<String>,
    available: bool,
}

#[tauri::command]
fn status(state: State<'_, DshState>) -> StatusSnapshot {
    // Distribution inspection starts login shells and can outlive the initial
    // Windows startup. Snapshot instances last so the response cannot carry a
    // stale `starting` state that overwrites a newer event in the frontend.
    let wsl_distributions = list_wsl_distribution_info();
    let instances = InstanceKind::ALL
        .into_iter()
        .map(|kind| state.snapshot(kind))
        .collect();
    StatusSnapshot {
        instances,
        wsl_distributions,
    }
}

#[tauri::command]
fn start_instance(
    kind: String,
    distro: Option<String>,
    handle: tauri::AppHandle,
    state: State<'_, DshState>,
) -> Result<InstanceSnapshot, String> {
    let kind = InstanceKind::parse(&kind)?;
    launch_instance(&handle, state.inner(), kind, distro.as_deref());
    Ok(state.snapshot(kind))
}

#[tauri::command]
fn stop_instance(
    kind: String,
    handle: tauri::AppHandle,
    state: State<'_, DshState>,
) -> Result<InstanceSnapshot, String> {
    let kind = InstanceKind::parse(&kind)?;
    stop_one_instance(&handle, state.inner(), kind);
    Ok(state.snapshot(kind))
}

#[tauri::command]
fn restart_instance(
    kind: String,
    distro: Option<String>,
    handle: tauri::AppHandle,
    state: State<'_, DshState>,
) -> Result<InstanceSnapshot, String> {
    let kind = InstanceKind::parse(&kind)?;
    stop_one_instance(&handle, state.inner(), kind);
    launch_instance(&handle, state.inner(), kind, distro.as_deref());
    Ok(state.snapshot(kind))
}

fn find_dsh() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let probe = Command::new("where.exe")
            .arg("dsh")
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&probe.stdout);
        let mut first = None;
        for raw in text.lines() {
            let path = PathBuf::from(raw.trim());
            if !path.is_file() {
                continue;
            }
            if first.is_none() {
                first = Some(path.clone());
            }
            if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("cmd"))
            {
                return Some(path);
            }
        }
        if let Some(path) = first {
            return Some(path);
        }
        let appdata = std::env::var_os("APPDATA")?;
        let path = PathBuf::from(appdata).join("npm").join("dsh.cmd");
        path.is_file().then_some(path)
    }
    #[cfg(not(windows))]
    {
        let probe = Command::new("sh")
            .args(["-lc", "command -v dsh"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&probe.stdout);
        let path = PathBuf::from(text.lines().next()?.trim());
        path.is_file().then_some(path)
    }
}

fn spawn_dsh(dsh: &Path) -> std::io::Result<Child> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let dir = dsh.parent().unwrap_or(Path::new("."));
        let name = dsh
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("dsh.cmd");
        let mut search_dirs = vec![dir.to_path_buf()];
        if let Some(current_path) = std::env::var_os("PATH") {
            search_dirs.extend(std::env::split_paths(&current_path));
        }
        let search_path = std::env::join_paths(search_dirs)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;

        Command::new("cmd.exe")
            .args(["/D", "/C", name, "web", "--port", "0"])
            .env("PATH", search_path)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::process::CommandExt;

        Command::new(dsh)
            .args(["web", "--port", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
    }
}

fn parse_version_output(text: &str) -> Option<String> {
    text.lines().map(str::trim).find_map(|line| {
        let version = line.strip_prefix('v').unwrap_or(line);
        let begins_with_digit = version
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit());
        let safe_version = version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+' | '_')
        });
        (begins_with_digit && safe_version).then(|| version.to_owned())
    })
}

fn probe_dsh_version(dsh: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let dir = dsh.parent().unwrap_or(Path::new("."));
        let name = dsh.file_name()?.to_str()?;
        let mut search_dirs = vec![dir.to_path_buf()];
        if let Some(current_path) = std::env::var_os("PATH") {
            search_dirs.extend(std::env::split_paths(&current_path));
        }
        let search_path = std::env::join_paths(search_dirs).ok()?;
        let output = Command::new("cmd.exe")
            .args(["/D", "/C", name, "--version"])
            .env("PATH", search_path)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        parse_version_output(&stdout).or_else(|| parse_version_output(&stderr))
    }
    #[cfg(not(windows))]
    {
        let output = Command::new(dsh).arg("--version").output().ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        parse_version_output(&stdout).or_else(|| parse_version_output(&stderr))
    }
}

fn is_linux_wsl_path(path: &str) -> bool {
    if !path.starts_with('/') {
        return false;
    }
    let Some(rest) = path.strip_prefix("/mnt/") else {
        return true;
    };
    let bytes = rest.as_bytes();
    !(bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b'/')
}

#[derive(Debug, Eq, PartialEq)]
struct WslDsh {
    distro: String,
    path: String,
    version: Option<String>,
}

fn decode_wsl_output(bytes: &[u8]) -> String {
    let has_bom = bytes.starts_with(&[0xff, 0xfe]);
    let pairs = bytes.len() / 2;
    let zero_high_bytes = bytes
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|byte| **byte == 0)
        .count();
    let looks_utf16 = has_bom || (pairs > 0 && zero_high_bytes * 2 >= pairs);
    if looks_utf16 {
        let start = usize::from(has_bom) * 2;
        let words: Vec<u16> = bytes[start..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&words)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(windows)]
fn list_wsl_distros() -> Result<Vec<String>, String> {
    use std::os::windows::process::CommandExt;

    let list = Command::new("wsl.exe")
        .args(["--list", "--quiet"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("无法列出 WSL 发行版：{error}"))?;
    let distros: Vec<String> = decode_wsl_output(&list.stdout)
        .lines()
        .map(|line| line.trim_matches(['\0', '\u{feff}', ' ', '\r']))
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    (!distros.is_empty())
        .then_some(distros)
        .ok_or_else(|| "未安装 WSL 发行版".to_string())
}

#[cfg(not(windows))]
fn list_wsl_distros() -> Result<Vec<String>, String> {
    Err("WSL 实例仅支持 Windows".to_string())
}

#[cfg(windows)]
fn probe_wsl_dsh_version(distro: &str, dsh: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;

    let output = Command::new("wsl.exe")
        .args([
            "--distribution",
            distro,
            "--exec",
            "bash",
            "-lic",
            "exec \"$1\" --version",
            "dsh-gui",
            dsh,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let stdout = decode_wsl_output(&output.stdout);
    let stderr = decode_wsl_output(&output.stderr);
    parse_version_output(&stdout).or_else(|| parse_version_output(&stderr))
}

#[cfg(windows)]
fn find_wsl_dsh_in_known_distro(distro: &str) -> Result<WslDsh, String> {
    use std::os::windows::process::CommandExt;

    let output = Command::new("wsl.exe")
        .args([
            "--distribution",
            distro,
            "--exec",
            "bash",
            "-lic",
            "type -aP dsh 2>/dev/null || true",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("检查 WSL 发行版 {distro} 失败：{error}"))?;
    let stdout = decode_wsl_output(&output.stdout);
    let path = stdout
        .lines()
        .map(str::trim)
        .find(|path| is_linux_wsl_path(path))
        .ok_or_else(|| {
            format!(
                "WSL 发行版 {distro} 中未找到 Linux 版 dsh；请在该发行版内执行 npm i -g @deepseek-ai/dsh"
            )
        })?;
    Ok(WslDsh {
        distro: distro.to_owned(),
        path: path.to_owned(),
        version: probe_wsl_dsh_version(distro, path),
    })
}

#[cfg(windows)]
fn find_wsl_dsh(distro: &str) -> Result<WslDsh, String> {
    if !list_wsl_distros()?
        .iter()
        .any(|candidate| candidate == distro)
    {
        return Err(format!("WSL 发行版不存在：{distro}"));
    }
    find_wsl_dsh_in_known_distro(distro)
}

#[cfg(not(windows))]
fn find_wsl_dsh(_distro: &str) -> Result<WslDsh, String> {
    Err("WSL 实例仅支持 Windows".to_string())
}

#[cfg(windows)]
fn list_wsl_distribution_info() -> Vec<WslDistributionInfo> {
    list_wsl_distros()
        .unwrap_or_default()
        .into_iter()
        .map(|name| match find_wsl_dsh_in_known_distro(&name) {
            Ok(dsh) => WslDistributionInfo {
                name,
                version: dsh.version,
                available: true,
            },
            Err(_) => WslDistributionInfo {
                name,
                version: None,
                available: false,
            },
        })
        .collect()
}

#[cfg(not(windows))]
fn list_wsl_distribution_info() -> Vec<WslDistributionInfo> {
    Vec::new()
}

fn free_loopback_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

#[cfg(windows)]
fn spawn_wsl_dsh(distro: &str, dsh: &str, port: u16) -> std::io::Result<Child> {
    use std::os::windows::process::CommandExt;

    Command::new("wsl.exe")
        .args([
            "--distribution",
            distro,
            "--exec",
            "bash",
            "-lic",
            "exec \"$1\" web --port \"$2\"",
            "dsh-gui",
            dsh,
            &port.to_string(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

#[cfg(not(windows))]
fn spawn_wsl_dsh(_distro: &str, _dsh: &str, _port: u16) -> std::io::Result<Child> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "WSL is only available on Windows",
    ))
}

fn kill_tree(child: &mut Child) {
    let pid = child.id();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let _ = Command::new("taskkill.exe")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill")
            .args(["-KILL", &format!("-{pid}")])
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn extract_url(line: &str) -> Option<String> {
    let marker = "dsh web: ";
    let rest = &line[line.find(marker)? + marker.len()..];
    let candidate = rest.split_whitespace().next()?;
    if !candidate.starts_with(URL_MARKER) {
        return None;
    }

    let parsed = url::Url::parse(candidate).ok()?;
    let valid = parsed.scheme() == "http"
        && parsed.host_str() == Some("127.0.0.1")
        && parsed.port().is_some_and(|port| port != 0)
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.path() == "/"
        && parsed.query().is_none()
        && parsed.fragment().is_none();
    valid.then(|| candidate.to_string())
}

fn launch_instance(
    handle: &tauri::AppHandle,
    state: &DshState,
    kind: InstanceKind,
    distro: Option<&str>,
) {
    let generation = {
        let mut runtime = state.runtime(kind).lock().unwrap();
        if matches!(
            runtime.phase,
            InstancePhase::Starting | InstancePhase::Ready
        ) {
            return;
        }
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.revision = runtime.revision.wrapping_add(1);
        runtime.phase = InstancePhase::Starting;
        runtime.url = None;
        runtime.detail = None;
        runtime.version = None;
        runtime.error = None;
        runtime.log.clear();
        runtime.generation
    };
    state.emit_snapshot(handle, kind);

    let spawned = match kind {
        InstanceKind::Windows => find_dsh()
            .ok_or_else(|| "未找到 Windows 全局 dsh；请执行 npm i -g @deepseek-ai/dsh".to_string())
            .and_then(|dsh| {
                let version = probe_dsh_version(&dsh);
                spawn_dsh(&dsh)
                    .map(|child| {
                        (
                            child,
                            format!("启动 {} web --port 0", dsh.display()),
                            Some("Windows".to_string()),
                            version,
                        )
                    })
                    .map_err(|error| format!("启动 Windows dsh 失败：{error}"))
            }),
        InstanceKind::Wsl => distro
            .filter(|distro| !distro.trim().is_empty())
            .ok_or_else(|| "请先在标题栏选择 WSL 发行版".to_string())
            .and_then(find_wsl_dsh)
            .and_then(|dsh| {
                let port =
                    free_loopback_port().map_err(|error| format!("分配 WSL 端口失败：{error}"))?;
                let version = dsh.version.clone();
                spawn_wsl_dsh(&dsh.distro, &dsh.path, port)
                    .map(|child| {
                        (
                            child,
                            format!("启动 WSL {}:{} web --port {port}", dsh.distro, dsh.path),
                            Some(dsh.distro),
                            version,
                        )
                    })
                    .map_err(|error| format!("启动 WSL dsh 失败：{error}"))
            }),
    };

    let (mut child, launch_log, detail, version) = match spawned {
        Ok(value) => value,
        Err(message) => {
            state.set_error(handle, kind, generation, message);
            return;
        }
    };

    let stdout = child.stdout.take().expect("dsh stdout is piped");
    let stderr = child.stderr.take().expect("dsh stderr is piped");
    {
        let mut runtime = state.runtime(kind).lock().unwrap();
        if runtime.generation != generation || runtime.phase != InstancePhase::Starting {
            drop(runtime);
            kill_tree(&mut child);
            return;
        }
        runtime.child = Some(child);
        runtime.detail = detail;
        runtime.version = version;
        runtime.revision = runtime.revision.wrapping_add(1);
    }
    state.push_log(handle, kind, generation, launch_log);
    spawn_stdout_watcher(handle.clone(), kind, generation, stdout);
    spawn_stderr_watcher(handle.clone(), kind, generation, stderr);
}

fn stop_one_instance(handle: &tauri::AppHandle, state: &DshState, kind: InstanceKind) {
    let child = {
        let mut runtime = state.runtime(kind).lock().unwrap();
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.revision = runtime.revision.wrapping_add(1);
        runtime.phase = InstancePhase::Stopped;
        runtime.url = None;
        runtime.error = None;
        runtime.log.clear();
        runtime.child.take()
    };
    state.emit_snapshot(handle, kind);
    if let Some(mut child) = child {
        kill_tree(&mut child);
    }
}

fn describe_exit(status: Option<ExitStatus>) -> String {
    status
        .map(|status| format!("dsh web 已退出（{status}）"))
        .unwrap_or_else(|| "dsh web 进程已结束".to_string())
}

fn spawn_stdout_watcher(
    handle: tauri::AppHandle,
    kind: InstanceKind,
    generation: u64,
    stdout: std::process::ChildStdout,
) {
    std::thread::spawn(move || {
        let state = handle.state::<DshState>();
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(_) => break,
            };
            if !state.push_log(&handle, kind, generation, line.clone()) {
                return;
            }
            if let Some(url) = extract_url(&line) {
                state.set_ready(&handle, kind, generation, url);
            }
        }

        let status = state
            .take_child(kind, generation)
            .and_then(|mut child| child.wait().ok());
        let existing_error = state.snapshot(kind).error;
        if existing_error.is_none() {
            state.set_error(&handle, kind, generation, describe_exit(status));
        }
    });
}

fn spawn_stderr_watcher(
    handle: tauri::AppHandle,
    kind: InstanceKind,
    generation: u64,
    stderr: std::process::ChildStderr,
) {
    std::thread::spawn(move || {
        let state = handle.state::<DshState>();
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(_) => break,
            };
            if !state.push_log(&handle, kind, generation, line) {
                break;
            }
        }
    });
}

pub fn run() {
    use tauri_plugin_window_state::StateFlags;

    let window_state_flags =
        StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED | StateFlags::VISIBLE;
    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(window_state_flags)
                .build(),
        )
        .manage(DshState::new())
        .invoke_handler(tauri::generate_handler![
            status,
            start_instance,
            stop_instance,
            restart_instance
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<DshState>();
            launch_instance(&handle, state.inner(), InstanceKind::Windows, None);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build the dsh GUI");

    app.run(|handle, event| {
        if let RunEvent::ExitRequested { .. } | RunEvent::Exit = event {
            let state = handle.state::<DshState>();
            for mut child in state.stop_all() {
                kill_tree(&mut child);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        decode_wsl_output, extract_url, is_linux_wsl_path, parse_version_output, InstanceKind,
    };

    #[test]
    fn extracts_plain_semantic_versions_from_command_output() {
        assert_eq!(
            parse_version_output("Welcome to Ubuntu\n0.1.0-rc.6\n"),
            Some("0.1.0-rc.6".to_string())
        );
        assert_eq!(
            parse_version_output("dsh version 0.1.0"),
            None,
            "descriptive log lines must not be mistaken for a version"
        );
    }

    #[test]
    fn extracts_the_dsh_web_readiness_url() {
        assert_eq!(
            extract_url("dsh web: http://127.0.0.1:43123"),
            Some("http://127.0.0.1:43123".to_string())
        );
        assert_eq!(
            extract_url("[boot] dsh web: http://127.0.0.1:43123\n"),
            Some("http://127.0.0.1:43123".to_string())
        );
    }

    #[test]
    fn rejects_unrelated_or_noncanonical_urls() {
        for line in [
            "plugin: http://127.0.0.1:43123",
            "dsh web: https://127.0.0.1:43123",
            "dsh web: http://localhost:43123",
            "dsh web: http://127.0.0.1:0",
            "dsh web: http://127.0.0.1:43123/path",
            "dsh web: http://127.0.0.1:43123?query",
            "dsh web: http://127.0.0.1:43123#fragment",
        ] {
            assert_eq!(extract_url(line), None, "accepted {line:?}");
        }
    }

    #[test]
    fn only_accepts_linux_native_wsl_launchers() {
        assert!(is_linux_wsl_path("/usr/local/bin/dsh"));
        assert!(is_linux_wsl_path(
            "/home/user/.nvm/versions/node/v24/bin/dsh"
        ));
        assert!(is_linux_wsl_path("/mnt/wsl/dsh"));
        assert!(!is_linux_wsl_path(
            "/mnt/c/Users/name/AppData/Roaming/npm/dsh"
        ));
        assert!(!is_linux_wsl_path("C:\\Users\\name\\dsh.cmd"));
        assert!(!is_linux_wsl_path("dsh"));
    }

    #[test]
    fn decodes_wsl_utf16_distribution_output() {
        let words: Vec<u16> = "Debian\r\nUbuntu\r\n".encode_utf16().collect();
        let bytes: Vec<u8> = words.into_iter().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode_wsl_output(&bytes), "Debian\r\nUbuntu\r\n");
        assert_eq!(decode_wsl_output(b"Debian\n"), "Debian\n");
    }

    #[test]
    fn parses_instance_ids_strictly() {
        assert_eq!(InstanceKind::parse("windows"), Ok(InstanceKind::Windows));
        assert_eq!(InstanceKind::parse("wsl"), Ok(InstanceKind::Wsl));
        assert!(InstanceKind::parse("linux").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn cmd_shim_inherits_the_gui_working_directory() {
        use super::spawn_dsh;
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before the Unix epoch")
            .as_nanos();
        let launcher_dir =
            std::env::temp_dir().join(format!("dsh gui launcher {} {nonce}", std::process::id()));
        fs::create_dir(&launcher_dir).expect("create fake launcher directory");
        let launcher = launcher_dir.join("dsh.cmd");
        fs::write(
            &launcher,
            "@echo off\r\necho CWD:%CD%\r\necho ARGS:%*\r\necho dsh web: http://127.0.0.1:43123\r\n",
        )
        .expect("write fake dsh.cmd");

        let output = spawn_dsh(&launcher)
            .expect("spawn fake dsh.cmd")
            .wait_with_output()
            .expect("wait for fake dsh.cmd");
        assert!(output.status.success(), "fake dsh.cmd failed: {output:?}");
        let stdout = String::from_utf8(output.stdout).expect("fake dsh output is UTF-8");
        let expected_cwd = std::env::current_dir().expect("read test working directory");

        assert!(
            stdout
                .lines()
                .any(|line| line == format!("CWD:{}", expected_cwd.display())),
            "fake dsh.cmd ran from the wrong directory: {stdout:?}"
        );
        assert!(
            stdout.lines().any(|line| line == "ARGS:web --port 0"),
            "fake dsh.cmd received the wrong arguments: {stdout:?}"
        );

        fs::remove_dir_all(&launcher_dir).expect("remove fake launcher directory");
    }
}
