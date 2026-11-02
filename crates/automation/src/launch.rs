//! ZIYNAT: start the desktop app for a bridge that finds nobody listening.
//!
//! Claude Desktop and Claude Code start `ziynat-cli mcp --bridge … --launch-app <app>` when they
//! start, which may be before ZIYNAT is open. Rather than failing every tool call until the user
//! opens it, the bridge starts the app once and waits for its control port (and, on a first run,
//! for the token file the app creates).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long to wait for a freshly started app to listen: a cold start with a GPU probe and a
/// font scan can take a while on a slow machine.
pub const LAUNCH_WAIT: Duration = Duration::from_secs(45);

/// Starts the app at most once per [`RELAUNCH_AFTER`], whatever the number of failing calls.
pub struct Launcher {
    app: PathBuf,
    last: Mutex<Option<Instant>>,
}

/// A later failure (the user quit the app) may start it again after this long.
pub const RELAUNCH_AFTER: Duration = Duration::from_secs(60);

impl Launcher {
    pub fn new(app: impl Into<PathBuf>) -> Self {
        Launcher { app: app.into(), last: Mutex::new(None) }
    }

    pub fn app(&self) -> &Path {
        &self.app
    }

    /// Start the app unless it was started less than [`RELAUNCH_AFTER`] ago. Returns whether a
    /// start was attempted now or recently (so waiting for it makes sense).
    pub fn ensure_started(&self) -> std::io::Result<bool> {
        let mut last = self.last.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if last.is_some_and(|t| t.elapsed() < RELAUNCH_AFTER) {
            return Ok(true);
        }
        spawn_detached(&self.app)?;
        *last = Some(Instant::now());
        Ok(true)
    }
}

/// Start `app` with no arguments, detached from this process: its own console and process group
/// on Windows, and never this process's stdio, which carries the MCP protocol.
pub fn spawn_detached(app: &Path) -> std::io::Result<()> {
    let mut cmd = Command::new(app);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(dir) = app.parent() {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: closing Claude doesn't close ZIYNAT.
        cmd.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    cmd.spawn().map(drop)
}

/// Wait until `path` exists (the app writes its token file at startup), up to `limit`.
pub fn wait_for_file(path: &Path, limit: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if path.is_file() && std::fs::metadata(path).is_ok_and(|m| m.len() > 0) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_app_is_an_error_not_a_panic() {
        let launcher = Launcher::new("/definitely/not/here/ziynat");
        assert!(launcher.ensure_started().is_err());
    }

    #[test]
    fn waiting_for_a_file_that_appears() {
        let dir = std::env::temp_dir().join(format!("ziynat-launch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("token");
        let _ = std::fs::remove_file(&file);
        let writer = {
            let file = file.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                std::fs::write(file, "x").unwrap();
            })
        };
        assert!(wait_for_file(&file, Duration::from_secs(5)));
        writer.join().unwrap();
        assert!(!wait_for_file(&dir.join("never"), Duration::from_millis(300)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
