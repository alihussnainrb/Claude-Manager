//! Running as an MSIX package (Microsoft Store build): identity, startup task, link handler.

use windows::ApplicationModel::Activation::ActivationKind;
use windows::ApplicationModel::{AppInstance, Package, StartupTask, StartupTaskState};
use windows::core::HSTRING;

/// Matches `TaskId` in packaging/msix/AppxManifest.xml.in.
const STARTUP_TASK_ID: &str = "ClaudeManagerStartup";

/// The package family name when running packaged, `None` for the classic installer build.
pub fn family_name() -> Option<String> {
    let pkg = Package::Current().ok()?;
    Some(pkg.Id().ok()?.FamilyName().ok()?.to_string_lossy())
}

pub fn is_packaged() -> bool {
    family_name().is_some()
}

/// `<family>!ClaudeManager`, matching the `Application Id` in the manifest.
pub fn app_user_model_id() -> Option<String> {
    family_name().map(|f| format!("{f}!ClaudeManager"))
}

/// True when Windows started us through the startup task (i.e. at sign-in).
pub fn launched_at_startup() -> bool {
    AppInstance::GetActivatedEventArgs()
        .and_then(|a| a.Kind())
        .is_ok_and(|k| k == ActivationKind::StartupTask)
}

fn startup_task() -> windows::core::Result<StartupTask> {
    StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK_ID))?.join()
}

/// Turn "start with Windows" on or off. Returns the resulting state (users can block it in
/// Task Manager / Settings, in which case enabling is refused).
pub fn set_startup(enabled: bool) -> windows::core::Result<bool> {
    let task = startup_task()?;
    if enabled {
        let state = task.RequestEnableAsync()?.join()?;
        Ok(matches!(
            state,
            StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy
        ))
    } else {
        task.Disable()?;
        Ok(false)
    }
}

/// Whether the claude:// default Windows chose is this package.
pub fn owns_url_choice() -> bool {
    let (Some(family), Some(progid)) = (family_name(), crate::registry::url_user_choice()) else {
        return false;
    };
    let app_key = format!(r"Software\Classes\{progid}\Application");
    crate::registry::get_string(&app_key, "AppUserModelID")
        .ok()
        .flatten()
        .is_some_and(|aumid| aumid.starts_with(&format!("{family}!")))
}
