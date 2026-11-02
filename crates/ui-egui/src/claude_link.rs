//! ZIYNAT ↔ Claude AI over MCP.
//!
//! The desktop app listens on a loopback control port (see `docs/control-protocol.md`) and
//! `ziynat-cli mcp --bridge` turns that channel into an MCP server that Claude Desktop and
//! Claude Code start over stdio. This module is the app side of that pairing:
//!
//! - [`STATS`]: live connection counters, fed by the control transport and by [`gate`];
//! - [`ClaudeLink`]: where this launch listens, which token file guards it and which CLI to
//!   register, installed once at startup with [`install`];
//! - [`register_claude_desktop`] / [`register_claude_code`]: one-click client registration;
//! - [`gate`]: the permission check every control request passes before it runs
//!   (Preferences › Integrations › Agent Permission and Blocked Commands);
//! - [`prefs_panel`] and [`status`]: the Preferences page and the status-bar indicator.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use egui::{Color32, RichText, Sense, vec2};
use photocraft_engine::prefs::Integrations;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;

/// The name ZIYNAT registers under in Claude's MCP server lists.
pub const SERVER_NAME: &str = "ziynat";

// ------------------------------------------------------------------------------- statistics

/// Live counters of the Claude connection, shared by the transport threads and the UI.
pub struct LinkStats {
    connections: AtomicUsize,
    requests: AtomicU64,
    denied: AtomicU64,
    /// Unix milliseconds of the last request, 0 before the first.
    last_ms: AtomicU64,
}

/// The process-wide counters (one control listener per process).
pub static STATS: LinkStats = LinkStats { connections: AtomicUsize::new(0), requests: AtomicU64::new(0), denied: AtomicU64::new(0), last_ms: AtomicU64::new(0) };

/// Counts one authenticated connection while it lives.
pub struct ConnectionGuard(&'static LinkStats);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.connections.fetch_sub(1, Ordering::AcqRel);
    }
}

/// What the status bar and the Preferences page show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub connections: usize,
    pub requests: u64,
    pub denied: u64,
    /// Seconds since the last request, `None` before the first.
    pub idle_s: Option<u64>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl LinkStats {
    /// A client authenticated: counted until the guard drops with its connection.
    pub fn connected(&'static self) -> ConnectionGuard {
        self.connections.fetch_add(1, Ordering::AcqRel);
        ConnectionGuard(self)
    }

    pub fn record_request(&self) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.last_ms.store(now_ms(), Ordering::Relaxed);
    }

    pub fn record_denied(&self) {
        self.denied.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Snapshot {
        let last = self.last_ms.load(Ordering::Relaxed);
        Snapshot {
            connections: self.connections.load(Ordering::Acquire),
            requests: self.requests.load(Ordering::Relaxed),
            denied: self.denied.load(Ordering::Relaxed),
            idle_s: (last != 0).then(|| now_ms().saturating_sub(last) / 1000),
        }
    }
}

// ------------------------------------------------------------------------------- the link

/// Where this launch listens for Claude, installed by the desktop app at startup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClaudeLink {
    pub port: u16,
    /// The token file Claude's MCP bridge reads (the app creates it on first launch).
    pub token_file: Option<PathBuf>,
    /// `ziynat-cli` (or `photocraft-cli`) beside the app, the command Claude starts.
    pub cli: Option<PathBuf>,
    /// The app itself, so the bridge can start it when Claude connects first.
    pub app_exe: Option<PathBuf>,
    pub read_root: Option<PathBuf>,
    pub write_root: Option<PathBuf>,
}

static LINK: OnceLock<ClaudeLink> = OnceLock::new();

/// Record where this launch listens (once, at startup). Without it the app isn't listening.
pub fn install(link: ClaudeLink) {
    let _ = LINK.set(link);
}

/// This launch's link, when it listens for Claude.
pub fn current() -> Option<&'static ClaudeLink> {
    LINK.get()
}

fn path_text(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

impl ClaudeLink {
    /// The arguments after the CLI: `mcp --bridge 127.0.0.1:<port> --control-token-file <file>
    /// [--launch-app <app>]`.
    pub fn mcp_args(&self) -> Vec<String> {
        let mut args = vec!["mcp".to_owned(), "--bridge".to_owned(), format!("127.0.0.1:{}", self.port)];
        if let Some(file) = &self.token_file {
            args.push("--control-token-file".into());
            args.push(path_text(file));
        }
        if let Some(app) = &self.app_exe {
            args.push("--launch-app".into());
            args.push(path_text(app));
        }
        args
    }

    /// The CLI to start, as text (the bare name when it wasn't found beside the app).
    pub fn command(&self) -> String {
        self.cli.as_deref().map_or_else(|| "ziynat-cli".to_owned(), path_text)
    }

    /// The `mcpServers` entry for Claude Desktop (and any MCP client using that format).
    pub fn server_entry(&self) -> Value {
        json!({"command": self.command(), "args": self.mcp_args()})
    }

    /// A complete client configuration with only ZIYNAT in it, for copying.
    pub fn config_snippet(&self) -> String {
        serde_json::to_string_pretty(&json!({"mcpServers": {SERVER_NAME: self.server_entry()}})).unwrap_or_default()
    }

    /// The `claude mcp add` arguments (after `claude`) registering ZIYNAT for every project.
    pub fn claude_code_args(&self) -> Vec<String> {
        let mut args: Vec<String> = ["mcp", "add", "--scope", "user", SERVER_NAME, "--"].iter().map(|s| (*s).to_owned()).collect();
        args.push(self.command());
        args.extend(self.mcp_args());
        args
    }

    /// [`Self::claude_code_args`] as one shell line, quoting arguments with spaces.
    pub fn claude_code_command(&self) -> String {
        let quote = |a: &str| if a.contains([' ', '\t', '"']) { format!("\"{}\"", a.replace('"', "\\\"")) } else { a.to_owned() };
        std::iter::once("claude".to_owned()).chain(self.claude_code_args().iter().map(|a| quote(a))).collect::<Vec<_>>().join(" ")
    }
}

// ------------------------------------------------------------------------------- Claude Desktop

/// Claude Desktop's configuration file for this user, from the environment `env`.
pub fn claude_desktop_config_path_from(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let some = |v: Option<String>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = if cfg!(windows) {
        some(env("APPDATA"))?.join("Claude")
    } else if cfg!(target_os = "macos") {
        some(env("HOME"))?.join("Library/Application Support/Claude")
    } else {
        some(env("XDG_CONFIG_HOME")).or_else(|| some(env("HOME")).map(|h| h.join(".config")))?.join("Claude")
    };
    Some(dir.join("claude_desktop_config.json"))
}

/// Claude Desktop's configuration file for the current user.
pub fn claude_desktop_config_path() -> Option<PathBuf> {
    claude_desktop_config_path_from(|k| std::env::var(k).ok())
}

/// `existing` (a Claude Desktop configuration, or `None` for a new file) with `entry` set as the
/// `ziynat` MCP server. Every other setting and server is kept. A file that isn't a JSON object
/// is refused rather than replaced.
pub fn merge_desktop_config(existing: Option<&str>, entry: &Value) -> Result<String, String> {
    let mut root = match existing.map(str::trim).filter(|t| !t.is_empty()) {
        None => Value::Object(Map::new()),
        Some(text) => serde_json::from_str::<Value>(text).map_err(|e| format!("the Claude Desktop configuration isn't valid JSON ({e}); fix or remove it first"))?,
    };
    let Some(obj) = root.as_object_mut() else {
        return Err("the Claude Desktop configuration isn't a JSON object".into());
    };
    let servers = obj.entry("mcpServers").or_insert_with(|| Value::Object(Map::new()));
    let Some(servers) = servers.as_object_mut() else {
        return Err("`mcpServers` in the Claude Desktop configuration isn't an object".into());
    };
    servers.insert(SERVER_NAME.to_owned(), entry.clone());
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

/// Whether the configuration at `path` already has a `ziynat` server equal to `entry`.
pub fn desktop_registered(path: &Path, entry: &Value) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .is_some_and(|v| v.get("mcpServers").and_then(|s| s.get(SERVER_NAME)) == Some(entry))
}

/// Add (or update) ZIYNAT in Claude Desktop's configuration at `path`. The previous file is kept
/// once as `<name>.ziynat-backup`, and the new one is written beside it and renamed over it, so a
/// failed write never leaves Claude Desktop with half a file.
pub fn register_claude_desktop_at(link: &ClaudeLink, path: &Path) -> Result<(), String> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let merged = merge_desktop_config(existing.as_deref(), &link.server_entry())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    if let Some(text) = &existing {
        let backup = path.with_extension("json.ziynat-backup");
        if !backup.exists() {
            std::fs::write(&backup, text).map_err(|e| format!("cannot back up {}: {e}", path.display()))?;
        }
    }
    let tmp = path.with_extension("json.ziynat-tmp");
    std::fs::write(&tmp, merged).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot replace {}: {e}", path.display())
    })
}

/// [`register_claude_desktop_at`] the current user's Claude Desktop configuration.
pub fn register_claude_desktop(link: &ClaudeLink) -> Result<PathBuf, String> {
    let path = claude_desktop_config_path().ok_or("cannot find this user's Claude Desktop settings folder")?;
    register_claude_desktop_at(link, &path)?;
    Ok(path)
}

// ------------------------------------------------------------------------------- Claude Code

/// The `claude` executables to try, in order (npm installs `claude.cmd` on Windows).
fn claude_programs() -> &'static [&'static str] {
    if cfg!(windows) { &["claude", "claude.cmd", "claude.exe"] } else { &["claude"] }
}

fn claude_command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashes up over the app.
        cmd.creation_flags(0x0800_0000);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd
}

/// Register ZIYNAT with Claude Code (`claude mcp add --scope user …`), replacing an earlier
/// registration. Blocks while `claude` runs: call it off the UI thread.
pub fn register_claude_code(link: &ClaudeLink) -> Result<String, String> {
    let mut last = String::from("Claude Code (`claude`) was not found on PATH");
    for program in claude_programs() {
        // A stale entry would make `add` fail; its absence is fine.
        let _ = claude_command(program).args(["mcp", "remove", "--scope", "user", SERVER_NAME]).output();
        match claude_command(program).args(link.claude_code_args()).output() {
            Ok(out) if out.status.success() => return Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned()),
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stderr);
                return Err(if text.trim().is_empty() { format!("claude exited with {}", out.status) } else { text.trim().to_owned() });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => last = format!("cannot run {program}: {e}"),
        }
    }
    Err(last)
}

// ------------------------------------------------------------------------------- permissions

/// Control methods that only look: allowed even with Agent Permission set to Read Only.
pub const READ_METHODS: &[&str] = &["engine.commands", "jobs.list", "ui.inspect", "ui.menu.list", "ui.screenshot"];

/// Methods that open, save or close files or the app: refused below Full.
const FILE_METHODS: &[&str] = &["app.open", "app.save", "app.quit", "ui.window.close"];

/// Command-id prefixes that read or write files or devices: refused below Full.
const FILE_COMMANDS: &[&str] = &["file.", "app."];

/// Engine commands that only report: allowed with Read Only through `engine.execute`.
const READ_COMMANDS: &[&str] = &["command.list", "document.inspect", "document.sampleColor", "jobs.list", "prefs.get", "help.systemInfo", "brush.get"];

/// The command id a request runs, for the methods that run one.
fn command_id<'a>(method: &str, params: &'a Value) -> Option<&'a str> {
    let key = match method {
        "engine.execute" => "command",
        "ui.menu.invoke" => "id",
        _ => return None,
    };
    params.get(key).and_then(Value::as_str)
}

/// The blocked-command entries of `list` (commas, semicolons or whitespace between them).
pub fn blocked_entries(list: &str) -> Vec<&str> {
    list.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|s| !s.is_empty()).collect()
}

fn matches_entry(id: &str, entry: &str) -> bool {
    if let Some(prefix) = entry.strip_suffix('*') { id.starts_with(prefix) } else if entry.ends_with('.') { id.starts_with(entry) } else { id == entry }
}

/// May an agent run `method` with `params` under `prefs`? `Err` carries the reason it is told.
///
/// Whatever the permission, an agent can't change the Integrations preferences that limit it.
pub fn check(prefs: &Integrations, method: &str, params: &Value) -> Result<(), String> {
    if !prefs.allow_agent_control {
        return Err("agent control is turned off in Edit › Preferences › Integrations (Allow Agent Control)".into());
    }
    let id = command_id(method, params);
    if let Some(id) = id {
        if id.starts_with("edit.preferences.integrations") {
            return Err("agents can't open the Integrations preferences that set their own permissions".into());
        }
        if matches!(id, "prefs.set" | "prefs.reset" | "prefs.import")
            && params.get("params").and_then(|p| p.get("path")).and_then(Value::as_str).is_none_or(|p| p.starts_with("integrations"))
        {
            return Err("agents can't change the Integrations preferences that set their own permissions".into());
        }
        if let Some(entry) = blocked_entries(&prefs.blocked_commands).into_iter().find(|e| matches_entry(id, e)) {
            return Err(format!("`{id}` is blocked for agents (Preferences › Integrations › Blocked Commands: `{entry}`)"));
        }
    }
    match prefs.agent_permission.as_str() {
        "readOnly" => {
            let reading = READ_METHODS.contains(&method) || (method == "engine.execute" && id.is_some_and(|id| READ_COMMANDS.contains(&id)));
            if !reading {
                return Err(format!("`{}` changes the app, and Agent Permission is Read Only (Preferences › Integrations)", id.unwrap_or(method)));
            }
        }
        "editOnly" => {
            if FILE_METHODS.contains(&method) || id.is_some_and(|id| FILE_COMMANDS.iter().any(|p| id.starts_with(p))) {
                return Err(format!("`{}` opens, saves or closes files, and Agent Permission is Edit Only (Preferences › Integrations)", id.unwrap_or(method)));
            }
        }
        _ => {}
    }
    Ok(())
}

/// [`check`] against the app's preferences, counting the request (and a refusal) in [`STATS`].
pub fn gate(app: &PhotocraftApp, method: &str, params: &Value) -> Result<(), String> {
    STATS.record_request();
    let result = check(&app.session.prefs().integrations, method, params);
    if result.is_err() {
        STATS.record_denied();
    }
    result
}

// ------------------------------------------------------------------------------- UI

/// The connection state shown to the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// This launch doesn't listen.
    Off,
    /// Listening, no client connected.
    Ready,
    /// This many clients connected.
    Connected(usize),
}

pub fn state() -> State {
    match current() {
        None => State::Off,
        Some(_) => match STATS.snapshot().connections {
            0 => State::Ready,
            n => State::Connected(n),
        },
    }
}

fn state_color(state: State, t: &Tokens) -> Color32 {
    match state {
        State::Off => t.text_faint,
        State::Ready => t.warning,
        State::Connected(_) => Color32::from_rgb(0x3c, 0xb3, 0x71),
    }
}

fn state_text(state: State) -> String {
    match state {
        State::Off => tl!("Claude AI: off").to_owned(),
        State::Ready => tl!("Claude AI: ready").to_owned(),
        State::Connected(n) => crate::i18n::fmt(tl!("Claude AI: connected ({n})"), &[("n", &n.to_string())]),
    }
}

fn dot(ui: &mut egui::Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, color);
}

/// The status-bar indicator: a dot and a word; a click opens Preferences › Integrations.
pub fn status(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if current().is_none() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let state = state();
    let snap = STATS.snapshot();
    let resp = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            dot(ui, state_color(state, &t));
            ui.label(RichText::new(state_text(state)).color(t.text_dim).size(12.0));
        })
        .response
        .interact(Sense::click());
    let tip = crate::i18n::fmt(
        tl!("{requests} requests from Claude, {denied} refused. Click for Claude AI settings."),
        &[("requests", &snap.requests.to_string()), ("denied", &snap.denied.to_string())],
    );
    if resp.on_hover_text(tip).clicked() {
        crate::shortcut_dispatch::dispatch(app, ui.ctx(), "edit.preferences.integrations");
    }
    ui.add_space(8.0);
    // Connections come and go without input: look again now and then while listening.
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(2));
}

/// The outcome of the last registration, shown under the buttons.
static MESSAGE: Mutex<Option<(bool, String)>> = Mutex::new(None);

fn set_message(ok: bool, text: String) {
    *MESSAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((ok, text));
}

/// The Claude AI group at the top of Preferences › Integrations.
pub fn prefs_panel(ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    ui.group(|ui| {
        ui.label(RichText::new(tl!("Claude AI (MCP)")).strong());
        let state = state();
        ui.horizontal(|ui| {
            dot(ui, state_color(state, &t));
            ui.label(state_text(state));
        });
        let Some(link) = current() else {
            ui.label(
                RichText::new(tl!("ZIYNAT isn't listening for Claude. Turn on Allow Agent Control and Claude Auto Start below, then restart ZIYNAT."))
                    .color(t.text_dim),
            );
            return;
        };
        let snap = STATS.snapshot();
        ui.label(
            RichText::new(crate::i18n::fmt(
                tl!("Listening on 127.0.0.1:{port} (this computer only) · {requests} requests · {denied} refused"),
                &[("port", &link.port.to_string()), ("requests", &snap.requests.to_string()), ("denied", &snap.denied.to_string())],
            ))
            .color(t.text_dim),
        );
        if let Some(file) = &link.token_file {
            ui.label(RichText::new(crate::i18n::fmt(tl!("Access token: {file}"), &[("file", &file.display().to_string())])).color(t.text_dim));
        }
        let folder = |p: &Option<PathBuf>| p.as_ref().map_or_else(|| tl!("none").to_owned(), |p| p.display().to_string());
        ui.label(
            RichText::new(crate::i18n::fmt(
                tl!("Files Claude may read: {read} · write: {write}"),
                &[("read", &folder(&link.read_root)), ("write", &folder(&link.write_root))],
            ))
            .color(t.text_dim),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let desktop_path = claude_desktop_config_path();
            let registered = desktop_path.as_deref().is_some_and(|p| desktop_registered(p, &link.server_entry()));
            let label = if registered { tl!("Claude Desktop: connected ✓ (update)") } else { tl!("Connect to Claude Desktop") };
            if ui.button(label).clicked() {
                match register_claude_desktop(link) {
                    Ok(path) => set_message(
                        true,
                        crate::i18n::fmt(tl!("Added to {file}. Quit and reopen Claude Desktop to load ZIYNAT."), &[("file", &path.display().to_string())]),
                    ),
                    Err(e) => set_message(false, e),
                }
            }
            if ui.button(tl!("Connect to Claude Code")).clicked() {
                set_message(true, tl!("Running claude mcp add…").to_owned());
                let link = link.clone();
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    match register_claude_code(&link) {
                        Ok(_) => set_message(true, tl!("Registered with Claude Code for every project. Start a new claude session and run /mcp.").to_owned()),
                        Err(e) => set_message(
                            false,
                            crate::i18n::fmt(tl!("Claude Code: {error}. Run this command yourself: {command}"), &[("error", &e), ("command", &link.claude_code_command())]),
                        ),
                    }
                    ctx.request_repaint();
                });
            }
            if ui.button(tl!("Copy MCP configuration")).clicked() {
                ui.ctx().copy_text(link.config_snippet());
                set_message(true, tl!("Copied: paste it into any MCP client's configuration.").to_owned());
            }
            if ui.button(tl!("Copy Claude Code command")).clicked() {
                ui.ctx().copy_text(link.claude_code_command());
                set_message(true, tl!("Copied: run it in a terminal.").to_owned());
            }
        });
        if let Some((ok, text)) = MESSAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone() {
            ui.label(RichText::new(text).color(if ok { t.text_dim } else { t.warning }));
        }
        ui.label(
            RichText::new(tl!("Claude starts ziynat-cli, which connects here and starts ZIYNAT when it isn't running. Agent Permission and Blocked Commands below apply at once; folders and the port apply at next launch."))
                .color(t.text_faint),
        );
    });
    ui.add_space(8.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs(permission: &str, blocked: &str) -> Integrations {
        Integrations { agent_permission: permission.into(), blocked_commands: blocked.into(), ..Integrations::default() }
    }

    fn exec(id: &str) -> Value {
        json!({"command": id, "params": {}})
    }

    #[test]
    fn full_permission_runs_everything_but_its_own_settings() {
        let p = prefs("full", "");
        assert!(check(&p, "engine.execute", &exec("filter.blur.gaussianBlur")).is_ok());
        assert!(check(&p, "app.save", &json!({})).is_ok());
        assert!(check(&p, "ui.menu.invoke", &json!({"id": "edit.preferences.integrations"})).is_err());
        assert!(check(&p, "engine.execute", &json!({"command": "prefs.set", "params": {"path": "integrations.agentPermission", "value": "full"}})).is_err());
        assert!(check(&p, "engine.execute", &json!({"command": "prefs.set", "params": {"path": "interface.theme", "value": "pro"}})).is_ok());
    }

    #[test]
    fn read_only_allows_looking_only() {
        let p = prefs("readOnly", "");
        assert!(check(&p, "ui.inspect", &json!({})).is_ok());
        assert!(check(&p, "ui.screenshot", &json!({})).is_ok());
        assert!(check(&p, "engine.execute", &exec("command.list")).is_ok());
        assert!(check(&p, "engine.execute", &exec("layer.new.layer")).is_err());
        assert!(check(&p, "ui.pointer", &json!({})).is_err());
    }

    #[test]
    fn edit_only_refuses_files() {
        let p = prefs("editOnly", "");
        assert!(check(&p, "engine.execute", &exec("layer.new.layer")).is_ok());
        assert!(check(&p, "engine.execute", &exec("file.saveAs")).is_err());
        assert!(check(&p, "ui.menu.invoke", &json!({"id": "file.print"})).is_err());
        assert!(check(&p, "app.quit", &json!({})).is_err());
    }

    #[test]
    fn blocked_commands_match_ids_and_prefixes() {
        let p = prefs("full", "file.print, image.crop;filter.*");
        assert!(check(&p, "engine.execute", &exec("file.print")).is_err());
        assert!(check(&p, "engine.execute", &exec("file.printOne")).is_ok(), "an exact entry isn't a prefix");
        assert!(check(&p, "engine.execute", &exec("image.crop")).is_err());
        assert!(check(&p, "engine.execute", &exec("filter.blur.gaussianBlur")).is_err());
        assert!(check(&p, "engine.execute", &exec("layer.new.layer")).is_ok());
    }

    #[test]
    fn control_off_refuses_all() {
        let p = Integrations { allow_agent_control: false, ..Integrations::default() };
        assert!(check(&p, "ui.inspect", &json!({})).is_err());
    }

    #[test]
    fn desktop_config_merge_keeps_other_servers_and_settings() {
        let entry = json!({"command": "C:\\ZIYNAT\\ziynat-cli.exe", "args": ["mcp"]});
        let merged = merge_desktop_config(Some(r#"{"globalShortcut":"Ctrl+Space","mcpServers":{"other":{"command":"x"}}}"#), &entry).unwrap();
        let v: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["globalShortcut"], "Ctrl+Space");
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
        assert_eq!(v["mcpServers"]["ziynat"], entry);
        let fresh: Value = serde_json::from_str(&merge_desktop_config(None, &entry).unwrap()).unwrap();
        assert_eq!(fresh["mcpServers"]["ziynat"], entry);
        assert!(merge_desktop_config(Some("not json"), &entry).is_err());
        assert!(merge_desktop_config(Some("[1]"), &entry).is_err());
    }

    #[test]
    fn registering_writes_backs_up_and_is_detected() {
        let dir = std::env::temp_dir().join(format!("ziynat-claude-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("Claude").join("claude_desktop_config.json");
        let link = ClaudeLink { port: 47_820, token_file: Some(dir.join("t.token")), cli: Some(dir.join("ziynat-cli")), ..ClaudeLink::default() };
        register_claude_desktop_at(&link, &path).unwrap();
        assert!(desktop_registered(&path, &link.server_entry()));
        register_claude_desktop_at(&link, &path).unwrap();
        assert!(path.with_extension("json.ziynat-backup").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_arguments_name_the_bridge_token_and_app() {
        let link = ClaudeLink {
            port: 47_820,
            token_file: Some(PathBuf::from("/c/t.token")),
            cli: Some(PathBuf::from("/Program Files/ZIYNAT/ziynat-cli")),
            app_exe: Some(PathBuf::from("/app/ZIYNAT")),
            ..ClaudeLink::default()
        };
        assert_eq!(link.mcp_args(), ["mcp", "--bridge", "127.0.0.1:47820", "--control-token-file", "/c/t.token", "--launch-app", "/app/ZIYNAT"]);
        assert!(link.claude_code_command().starts_with("claude mcp add --scope user ziynat -- \"/Program Files/ZIYNAT/ziynat-cli\" mcp"));
    }

    #[test]
    fn desktop_config_path_follows_the_platform() {
        let p = claude_desktop_config_path_from(|k| match k {
            "APPDATA" => Some("C:\\Users\\a\\AppData\\Roaming".into()),
            "HOME" => Some("/home/a".into()),
            _ => None,
        })
        .unwrap();
        assert!(p.ends_with("claude_desktop_config.json"));
        assert!(p.parent().unwrap().ends_with("Claude"));
    }
}
