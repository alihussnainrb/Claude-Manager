//! Sign-in routing: own the `claude://` scheme and pass each callback to the profile that asked.

use cm_core::routing::{self, RouteDecision, RoutingInput};
use cm_core::{ProfileId, now_millis};
use cm_win::registry;

use crate::events::{AppEvent, EventTx};
use crate::model::AppModel;

/// The handler command pointing at this executable.
pub fn our_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\" --deeplink \"%1\"", exe.display()))
}

fn is_ours(cmd: &str) -> bool {
    our_command().is_some_and(|ours| cmd.eq_ignore_ascii_case(&ours))
}

/// Point `claude://` at us when routing is on. Returns true if the handler had to be changed.
pub fn ensure(model: &mut AppModel) -> bool {
    if !model.settings.deep_link_broker {
        return false;
    }
    let Some(ours) = our_command() else {
        return false;
    };
    let current = registry::protocol_handler().ok().flatten();
    if current.as_deref().is_some_and(is_ours) {
        return false;
    }
    if let Some(cur) = current.filter(|c| !c.contains("--deeplink")) {
        model.settings.saved_protocol_handler = Some(cur);
        model.save_settings();
    }
    match registry::set_protocol_handler(&ours) {
        Ok(()) => {
            tracing::info!("claude:// handler now points at Claude Manager");
            true
        }
        Err(e) => {
            model.notify_error(format!("Couldn't take over sign-in links: {e}"));
            false
        }
    }
}

/// Give `claude://` back to Claude Desktop.
pub fn restore(model: &mut AppModel) {
    let fallback = model
        .install
        .as_ref()
        .map(|i| format!("\"{}\" \"%1\"", i.exe.display()));
    let Some(cmd) = model.settings.saved_protocol_handler.clone().or(fallback) else {
        model.notify_error("Claude Desktop wasn't found, so its handler couldn't be restored.");
        return;
    };
    match registry::set_protocol_handler(&cmd) {
        Ok(()) => model.notify_info("Sign-in links go straight to Claude again."),
        Err(e) => model.notify_error(format!("Couldn't restore Claude's handler: {e}")),
    }
}

pub fn handler_is_ours() -> bool {
    registry::protocol_handler()
        .ok()
        .flatten()
        .as_deref()
        .is_some_and(is_ours)
}

/// Watch the handler key; Claude rewrites it when it starts.
pub fn spawn_watcher(tx: EventTx) {
    std::thread::Builder::new()
        .name("cm-protocol".into())
        .spawn(move || {
            loop {
                match registry::wait_for_change(registry::PROTOCOL_KEY) {
                    Ok(()) => {
                        if tx.send_blocking(AppEvent::ProtocolChanged).is_err() {
                            return;
                        }
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_secs(2)),
                }
            }
        })
        .expect("spawn protocol watcher");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Account,
    Connector,
}

pub fn link_kind(url: &str) -> LinkKind {
    if url.to_ascii_lowercase().contains("mcp") {
        LinkKind::Connector
    } else {
        LinkKind::Account
    }
}

/// Decide where `url` goes. Delivers it directly when clear; otherwise returns the candidates
/// to offer in the router window.
pub fn route(model: &mut AppModel, url: &str) -> Option<Vec<ProfileId>> {
    let input = RoutingInput {
        now: now_millis(),
        running: model.running_ids(),
        pending_sign_in: model.pending_sign_in.clone(),
        focus_history: model.focus_history.clone(),
    };
    match routing::decide(&input) {
        RouteDecision::Profile { id, reason } => {
            tracing::info!("routing {} to {id} ({reason:?})", redact(url));
            model.handoff(id, Some(url));
            None
        }
        RouteDecision::Ask(candidates)
            if !model.settings.ask_when_unclear && !candidates.is_empty() =>
        {
            model.handoff(candidates[0], Some(url));
            None
        }
        RouteDecision::Ask(candidates) if candidates.is_empty() => {
            // Nothing running: offer every profile; picking one starts it with the link.
            Some(model.profiles.iter().map(|p| p.id).collect())
        }
        RouteDecision::Ask(candidates) => Some(candidates),
    }
}

/// Log-safe form of a callback URL (drops the query, which carries codes and state).
pub fn redact(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}
