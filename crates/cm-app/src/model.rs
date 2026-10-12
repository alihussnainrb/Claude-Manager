//! The app's single source of truth: profiles, settings, and what's running.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use cm_core::paths::{Paths, normalize};
use cm_core::{
    Profile, ProfileColor, ProfileIcon, ProfileId, Settings, UnixMillis, now_millis, store,
};
use cm_win::appx::ClaudeInstall;
use cm_win::window::WindowHandle;
use gpui_kit::*;

use crate::events::{AppEvent, EventTx, Instance, ScanResult};

/// Restart at most this many times within [`CRASH_WINDOW_MS`].
const MAX_RESTARTS: usize = 3;
const CRASH_WINDOW_MS: i64 = 5 * 60 * 1000;
/// A launch that hasn't shown up in a scan yet counts as starting for this long.
const STARTING_GRACE_MS: i64 = 20_000;
/// How long Quit waits for Claude to close before ending the process.
const QUIT_GRACE: Duration = Duration::from_secs(4);
const FOCUS_HISTORY_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Running { since: UnixMillis },
    Starting,
    Restarting { attempt: usize },
    Crashed,
    Stopped,
}

impl Status {
    pub fn is_running(self) -> bool {
        matches!(self, Status::Running { .. })
    }
}

#[derive(Debug, Clone)]
pub struct Runtime {
    pub pid: u32,
    pub started_at: UnixMillis,
    pub windows: Vec<WindowHandle>,
}

/// Lifecycle bookkeeping for a profile we launched.
#[derive(Debug, Default, Clone)]
struct Life {
    pid: Option<u32>,
    launched_at: Option<UnixMillis>,
    stop_requested: bool,
    crashes: Vec<UnixMillis>,
    restarting: Option<usize>,
    crashed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub pid: u32,
    pub user_data_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Notice {
    pub text: SharedString,
    pub error: bool,
    pub at: UnixMillis,
}

pub struct AppModel {
    pub paths: Paths,
    pub profiles: Vec<Profile>,
    pub settings: Settings,
    pub install: Option<ClaudeInstall>,
    pub runtime: HashMap<ProfileId, Runtime>,
    pub orphans: Vec<Orphan>,
    pub selected: ProfileId,
    pub scanned_once: bool,
    pub notice: Option<Notice>,
    pub folder_sizes: HashMap<ProfileId, u64>,
    pub protocol_reclaims: u32,
    pub tagged: crate::taskbar::Tagged,
    ignored_orphans: HashSet<String>,
    life: HashMap<ProfileId, Life>,
    pub focus_history: Vec<(ProfileId, UnixMillis)>,
    pub pending_sign_in: Vec<(ProfileId, UnixMillis)>,
    tx: EventTx,
}

impl AppModel {
    pub fn load(paths: Paths, tx: EventTx) -> Self {
        let profiles = store::load_profiles(&paths).unwrap_or_else(|e| {
            tracing::error!("profiles.json unreadable, starting with Default only: {e}");
            vec![store::default_profile(&paths)]
        });
        let settings = store::load_settings(&paths).unwrap_or_else(|e| {
            tracing::error!("settings.json unreadable, using defaults: {e}");
            Settings::default()
        });
        let selected = profiles.first().map(|p| p.id).unwrap_or(ProfileId::DEFAULT);
        Self {
            paths,
            profiles,
            settings,
            install: None,
            runtime: HashMap::new(),
            orphans: Vec::new(),
            selected,
            scanned_once: false,
            notice: None,
            folder_sizes: HashMap::new(),
            protocol_reclaims: 0,
            tagged: Default::default(),
            ignored_orphans: HashSet::new(),
            life: HashMap::new(),
            focus_history: Vec::new(),
            pending_sign_in: Vec::new(),
            tx,
        }
    }

    // ---- queries ----------------------------------------------------------------------------

    pub fn profile(&self, id: ProfileId) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn selected_profile(&self) -> Option<&Profile> {
        self.profile(self.selected)
            .or_else(|| self.profiles.first())
    }

    pub fn data_dir(&self, p: &Profile) -> PathBuf {
        self.paths
            .effective_user_data_dir(p.user_data_dir.as_deref())
    }

    pub fn status(&self, id: ProfileId) -> Status {
        if let Some(rt) = self.runtime.get(&id) {
            return Status::Running {
                since: rt.started_at,
            };
        }
        let Some(life) = self.life.get(&id) else {
            return Status::Stopped;
        };
        if let Some(attempt) = life.restarting {
            return Status::Restarting { attempt };
        }
        if life.crashed {
            return Status::Crashed;
        }
        if life.pid.is_some()
            && life
                .launched_at
                .is_some_and(|t| now_millis() - t < STARTING_GRACE_MS)
        {
            return Status::Starting;
        }
        Status::Stopped
    }

    pub fn running_ids(&self) -> Vec<ProfileId> {
        self.profiles
            .iter()
            .map(|p| p.id)
            .filter(|id| self.runtime.contains_key(id))
            .collect()
    }

    pub fn running_count(&self) -> usize {
        self.runtime.len()
    }

    pub fn crashes_this_week(&self, id: ProfileId) -> usize {
        let week = 7 * 24 * 60 * 60 * 1000;
        self.life.get(&id).map_or(0, |l| {
            l.crashes
                .iter()
                .filter(|t| now_millis() - **t < week)
                .count()
        })
    }

    /// Profiles that share a signed-in account with another profile.
    pub fn duplicate_of(&self, id: ProfileId) -> Option<&Profile> {
        let me = self.profile(id)?;
        let acc = me.cached_account_uuid.as_ref()?;
        self.profiles
            .iter()
            .find(|p| p.id != id && p.cached_account_uuid.as_ref() == Some(acc))
    }

    fn profile_for_dir(&self, dir: Option<&Path>) -> Option<ProfileId> {
        let target = normalize(&self.paths.effective_user_data_dir(dir));
        self.profiles
            .iter()
            .find(|p| normalize(&self.data_dir(p)) == target)
            .map(|p| p.id)
    }

    pub fn profile_for_pid(&self, pid: u32) -> Option<ProfileId> {
        self.runtime
            .iter()
            .find(|(_, rt)| rt.pid == pid)
            .map(|(id, _)| *id)
    }

    // ---- notices ----------------------------------------------------------------------------

    pub fn notify_info(&mut self, text: impl Into<SharedString>) {
        self.notice = Some(Notice {
            text: text.into(),
            error: false,
            at: now_millis(),
        });
    }

    pub fn notify_error(&mut self, text: impl Into<SharedString>) {
        let text = text.into();
        tracing::warn!("{text}");
        self.notice = Some(Notice {
            text,
            error: true,
            at: now_millis(),
        });
    }

    // ---- persistence ------------------------------------------------------------------------

    pub fn save_profiles(&mut self) {
        if let Err(e) = store::save_profiles(&self.paths, &self.profiles) {
            self.notify_error(format!("Couldn't save profiles: {e}"));
        }
    }

    pub fn save_settings(&mut self) {
        if let Err(e) = store::save_settings(&self.paths, &self.settings) {
            self.notify_error(format!("Couldn't save settings: {e}"));
        }
    }

    // ---- lifecycle --------------------------------------------------------------------------

    fn launch_args(&self, p: &Profile) -> (Vec<String>, Vec<(String, String)>) {
        let mut args = Vec::new();
        if let Some(dir) = &p.user_data_dir {
            args.push(format!("--user-data-dir={}", dir.display()));
        }
        args.extend(p.extra_args.iter().cloned());
        let mut env = Vec::new();
        if p.claude_code.enabled {
            env.push((
                "CLAUDE_CONFIG_DIR".to_string(),
                p.claude_code.config_dir.display().to_string(),
            ));
        }
        (args, env)
    }

    /// Start a profile, or bring it forward if it's already running.
    pub fn launch(&mut self, id: ProfileId) {
        if self.runtime.contains_key(&id) {
            self.show(id);
            return;
        }
        let Some(p) = self.profile(id).cloned() else {
            return;
        };
        let Some(install) = self.install.clone() else {
            self.notify_error("Claude Desktop isn't installed. Install it from the Microsoft Store, then try again.");
            return;
        };
        if p.claude_code.enabled {
            let _ = std::fs::create_dir_all(&p.claude_code.config_dir);
        }
        let (args, env) = self.launch_args(&p);
        match cm_win::process::spawn(&install.exe, &args, &env) {
            Ok(mut child) => {
                let pid = child.id();
                let life = self.life.entry(id).or_default();
                life.pid = Some(pid);
                life.launched_at = Some(now_millis());
                life.stop_requested = false;
                life.crashed = false;
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let code = child.wait().ok().and_then(|s| s.code());
                    let _ = tx.send_blocking(AppEvent::Exited { id, pid, code });
                });
                if let Some(p) = self.profiles.iter_mut().find(|p| p.id == id) {
                    p.last_launched_at = Some(now_millis());
                }
                self.save_profiles();
                tracing::info!("launched {} (pid {pid})", p.name);
            }
            Err(e) => self.notify_error(format!("Couldn't start Claude for {}: {e}", p.name)),
        }
    }

    /// Bring a running profile's window forward; launches it if it isn't running.
    pub fn show(&mut self, id: ProfileId) {
        let Some(rt) = self.runtime.get(&id).cloned() else {
            self.launch(id);
            return;
        };
        let visible = rt.windows.iter().copied().find(|w| w.is_alive());
        let focused = visible.is_some_and(cm_win::window::focus);
        if !focused {
            // Hidden in Claude's own tray: a second launch with the same folder makes the
            // running instance show its window, then exits.
            self.handoff(id, None);
        }
        self.note_focus(id);
    }

    /// Run Claude for `id` with an optional URL; a running instance receives it and the
    /// helper process exits.
    pub fn handoff(&mut self, id: ProfileId, url: Option<&str>) {
        let (Some(p), Some(install)) = (self.profile(id).cloned(), self.install.clone()) else {
            return;
        };
        let (mut args, env) = self.launch_args(&p);
        if let Some(url) = url {
            args.push(url.to_string());
        }
        if let Err(e) = cm_win::process::spawn(&install.exe, &args, &env) {
            self.notify_error(format!("Couldn't reach Claude for {}: {e}", p.name));
        }
    }

    /// Close a profile's Claude. Asks politely first, then ends the process.
    pub fn quit(&mut self, id: ProfileId) {
        let life = self.life.entry(id).or_default();
        life.stop_requested = true;
        life.restarting = None;
        let Some(rt) = self.runtime.get(&id).cloned() else {
            return;
        };
        for w in &rt.windows {
            cm_win::window::request_close(*w);
        }
        let pid = rt.pid;
        std::thread::spawn(move || {
            std::thread::sleep(QUIT_GRACE);
            if cm_win::process::is_running(pid) {
                let _ = cm_win::process::terminate(pid, 0);
            }
        });
    }

    pub fn stop_restarting(&mut self, id: ProfileId) {
        let life = self.life.entry(id).or_default();
        life.restarting = None;
        life.stop_requested = true;
        if let Some(rt) = self.runtime.get(&id).cloned() {
            let _ = cm_win::process::terminate(rt.pid, 0);
        }
    }

    /// A process we launched ended. Returns a profile to relaunch after a short delay.
    pub fn on_exit(&mut self, id: ProfileId, pid: u32, code: Option<i32>) -> Option<ProfileId> {
        let auto_restart = self.profile(id).is_some_and(|p| p.auto_restart);
        let life = self.life.entry(id).or_default();
        if life.pid != Some(pid) {
            return None; // an older launch, or a hand-off helper
        }
        life.pid = None;
        let clean = code == Some(0) || code.is_none() && life.stop_requested;
        if life.stop_requested || clean {
            life.restarting = None;
            return None;
        }
        let now = now_millis();
        life.crashes.push(now);
        let recent = life
            .crashes
            .iter()
            .filter(|t| now - **t < CRASH_WINDOW_MS)
            .count();
        tracing::warn!("profile {id} exited with {code:?} ({recent} crashes in window)");
        if auto_restart && recent <= MAX_RESTARTS {
            life.restarting = Some(recent);
            Some(id)
        } else {
            life.restarting = None;
            life.crashed = true;
            None
        }
    }

    pub fn relaunch_after_crash(&mut self, id: ProfileId) {
        let still_wanted = self
            .life
            .get(&id)
            .is_some_and(|l| l.restarting.is_some() && !l.stop_requested);
        if still_wanted && !self.runtime.contains_key(&id) {
            self.launch(id);
            if let Some(l) = self.life.get_mut(&id) {
                // Keep showing "Restarting" until the scan sees it running.
                if l.pid.is_none() {
                    l.restarting = None;
                    l.crashed = true;
                }
            }
        }
    }

    // ---- scanning ---------------------------------------------------------------------------

    pub fn apply_scan(&mut self, scan: ScanResult) {
        self.install = scan.install;
        let mut runtime = HashMap::new();
        let mut orphans = Vec::new();
        let mut accounts_changed = false;
        for Instance {
            pid,
            user_data_dir,
            windows,
            started_at,
            account,
        } in scan.instances
        {
            match self.profile_for_dir(user_data_dir.as_deref()) {
                Some(id) => {
                    if let Some(acc) = account
                        && let Some(p) = self.profiles.iter_mut().find(|p| p.id == id)
                        && p.cached_account_uuid.as_deref() != Some(acc.as_str())
                    {
                        p.cached_account_uuid = Some(acc);
                        accounts_changed = true;
                    }
                    let started_at = started_at.unwrap_or_else(now_millis);
                    runtime.insert(
                        id,
                        Runtime {
                            pid,
                            started_at,
                            windows,
                        },
                    );
                }
                None => {
                    let dir = user_data_dir.unwrap_or_else(|| self.paths.default_user_data_dir());
                    if !self.ignored_orphans.contains(&normalize(&dir)) {
                        orphans.push(Orphan {
                            pid,
                            user_data_dir: dir,
                        });
                    }
                }
            }
        }
        for id in runtime.keys() {
            if let Some(l) = self.life.get_mut(id) {
                l.restarting = None;
                l.crashed = false;
            }
        }
        // Signed in now: no longer waiting for a sign-in callback.
        self.pending_sign_in.retain(|(id, _)| {
            self.profiles
                .iter()
                .find(|p| p.id == *id)
                .is_none_or(|p| p.cached_account_uuid.is_none())
        });
        self.runtime = runtime;
        self.orphans = orphans;
        self.scanned_once = true;
        if accounts_changed {
            self.save_profiles();
        }
    }

    pub fn note_focus(&mut self, id: ProfileId) {
        if self
            .focus_history
            .last()
            .is_some_and(|(last, _)| *last == id)
        {
            return;
        }
        self.focus_history.push((id, now_millis()));
        if self.focus_history.len() > FOCUS_HISTORY_LEN {
            self.focus_history.remove(0);
        }
    }

    pub fn mark_pending_sign_in(&mut self, id: ProfileId) {
        self.pending_sign_in.retain(|(p, _)| *p != id);
        self.pending_sign_in.push((id, now_millis()));
    }

    // ---- editing ----------------------------------------------------------------------------

    pub fn create_profile(
        &mut self,
        name: &str,
        icon: ProfileIcon,
        color: ProfileColor,
        isolate_code: bool,
    ) -> ProfileId {
        let mut p = store::new_profile(&self.paths, name, &self.profiles);
        p.icon = icon;
        p.color = color;
        p.claude_code.enabled = isolate_code;
        let id = p.id;
        self.profiles.push(p);
        self.selected = id;
        self.save_profiles();
        id
    }

    pub fn update_profile(&mut self, id: ProfileId, f: impl FnOnce(&mut Profile)) {
        if let Some(p) = self.profiles.iter_mut().find(|p| p.id == id) {
            f(p);
            self.save_profiles();
        }
    }

    /// Remove from the list. Folders stay on disk unless `recycle` is set, in which case they
    /// go to the Recycle Bin.
    pub fn remove_profile(&mut self, id: ProfileId, recycle: bool) {
        if id.is_default() {
            return;
        }
        let Some(idx) = self.profiles.iter().position(|p| p.id == id) else {
            return;
        };
        if self.runtime.contains_key(&id) {
            self.notify_error("Quit this profile's Claude before removing it.");
            return;
        }
        let p = self.profiles.remove(idx);
        if recycle {
            for dir in [
                p.user_data_dir.clone(),
                p.claude_code
                    .enabled
                    .then(|| p.claude_code.config_dir.clone()),
            ]
            .into_iter()
            .flatten()
            {
                if let Err(e) = cm_win::shell::recycle(&dir) {
                    self.notify_error(format!("{e}"));
                }
            }
        }
        if self.selected == id {
            self.selected = self
                .profiles
                .first()
                .map(|p| p.id)
                .unwrap_or(ProfileId::DEFAULT);
        }
        self.save_profiles();
        self.notify_info(format!("Removed {}", p.name));
    }

    /// Turn an unattached instance's folder into a profile.
    pub fn adopt(&mut self, dir: &Path) -> ProfileId {
        let name = dir
            .file_name()
            .map(|n| {
                n.to_string_lossy()
                    .trim_start_matches("Claude-")
                    .to_string()
            })
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Adopted".into());
        let mut p = store::new_profile(&self.paths, &name, &self.profiles);
        p.name = title_case(&name);
        p.user_data_dir = Some(dir.to_path_buf());
        p.claude_code.enabled = false;
        let id = p.id;
        self.profiles.push(p);
        self.selected = id;
        self.save_profiles();
        id
    }

    pub fn ignore_orphan(&mut self, dir: &Path) {
        self.ignored_orphans.insert(normalize(dir));
        self.orphans
            .retain(|o| normalize(&o.user_data_dir) != normalize(dir));
    }

    pub fn update_settings(&mut self, f: impl FnOnce(&mut Settings)) {
        f(&mut self.settings);
        self.save_settings();
    }

    pub fn refresh_folder_size(&self, id: ProfileId) {
        if let Some(p) = self.profile(id) {
            crate::scan::spawn_folder_size(self.tx.clone(), id, self.data_dir(p));
        }
    }
}

fn title_case(s: &str) -> String {
    s.split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// "3h 12m", "48m", "just now".
pub fn format_uptime(since: UnixMillis) -> String {
    let mins = (now_millis() - since).max(0) / 60_000;
    match mins {
        0 => "just now".into(),
        m if m < 60 => format!("{m}m"),
        m if m < 24 * 60 => format!("{}h {}m", m / 60, m % 60),
        m => format!("{}d {}h", m / (24 * 60), (m / 60) % 24),
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mb = bytes as f64 / MB;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else if mb >= 10.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{mb:.1} MB")
    }
}

/// Global handle to the model so window code and event handlers can reach it.
pub struct ModelHandle(pub Entity<AppModel>);
impl Global for ModelHandle {}

pub fn model(cx: &App) -> Entity<AppModel> {
    cx.global::<ModelHandle>().0.clone()
}
