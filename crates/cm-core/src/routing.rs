//! Which profile should receive a `claude://` callback.
//!
//! When several Claude instances are running, Windows hands the callback to whichever process
//! owns the scheme, so sign-ins land in the wrong account. The broker asks this module where the
//! URL really belongs.

use crate::UnixMillis;
use crate::profile::ProfileId;

/// A profile the user explicitly started a sign-in for (or that launched without an account).
pub const PENDING_SIGN_IN_WINDOW_MS: i64 = 10 * 60 * 1000;
/// How recent a foreground event must be to count as "the profile the user is working in".
pub const RECENT_FOCUS_WINDOW_MS: i64 = 30 * 60 * 1000;

#[derive(Debug, Clone, Default)]
pub struct RoutingInput {
    pub now: UnixMillis,
    /// Profiles currently running.
    pub running: Vec<ProfileId>,
    /// `(profile, since)` for profiles awaiting a sign-in callback.
    pub pending_sign_in: Vec<(ProfileId, UnixMillis)>,
    /// `(profile, at)` for the most recent foreground events, newest last.
    pub focus_history: Vec<(ProfileId, UnixMillis)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    /// Deliver to this profile.
    Profile { id: ProfileId, reason: RouteReason },
    /// Ambiguous: ask the user, offering these candidates (most likely first).
    Ask(Vec<ProfileId>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteReason {
    PendingSignIn,
    RecentFocus,
    OnlyRunning,
}

pub fn decide(input: &RoutingInput) -> RouteDecision {
    let running = |id: &ProfileId| input.running.contains(id);

    // 1. Most recent pending sign-in that is still running and fresh.
    let pending = input
        .pending_sign_in
        .iter()
        .filter(|(id, since)| running(id) && input.now - since <= PENDING_SIGN_IN_WINDOW_MS)
        .max_by_key(|(_, since)| *since);
    if let Some((id, _)) = pending {
        return RouteDecision::Profile {
            id: *id,
            reason: RouteReason::PendingSignIn,
        };
    }

    // 2. Only one instance running: no ambiguity.
    if let [only] = input.running.as_slice() {
        return RouteDecision::Profile {
            id: *only,
            reason: RouteReason::OnlyRunning,
        };
    }

    // 3. The profile the user was last working in.
    let recent = input
        .focus_history
        .iter()
        .rev()
        .find(|(id, at)| running(id) && input.now - at <= RECENT_FOCUS_WINDOW_MS);
    if let Some((id, _)) = recent {
        return RouteDecision::Profile {
            id: *id,
            reason: RouteReason::RecentFocus,
        };
    }

    // 4. Ask, ordering candidates by recency of focus.
    let mut candidates: Vec<ProfileId> = Vec::new();
    for (id, _) in input.focus_history.iter().rev() {
        if running(id) && !candidates.contains(id) {
            candidates.push(*id);
        }
    }
    for id in &input.running {
        if !candidates.contains(id) {
            candidates.push(*id);
        }
    }
    RouteDecision::Ask(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (ProfileId, ProfileId, ProfileId) {
        (ProfileId::new(), ProfileId::new(), ProfileId::new())
    }

    #[test]
    fn pending_wins_over_focus() {
        let (a, b, _) = ids();
        let d = decide(&RoutingInput {
            now: 1_000_000,
            running: vec![a, b],
            pending_sign_in: vec![(b, 990_000)],
            focus_history: vec![(a, 999_000)],
        });
        assert_eq!(
            d,
            RouteDecision::Profile {
                id: b,
                reason: RouteReason::PendingSignIn
            }
        );
    }

    #[test]
    fn newest_pending_wins() {
        let (a, b, _) = ids();
        let d = decide(&RoutingInput {
            now: 1_000_000,
            running: vec![a, b],
            pending_sign_in: vec![(a, 900_000), (b, 950_000)],
            ..Default::default()
        });
        assert_eq!(
            d,
            RouteDecision::Profile {
                id: b,
                reason: RouteReason::PendingSignIn
            }
        );
    }

    #[test]
    fn stale_or_stopped_pending_ignored() {
        let (a, b, c) = ids();
        let now = 100 * 60 * 1000;
        let d = decide(&RoutingInput {
            now,
            running: vec![a, b],
            pending_sign_in: vec![(a, now - PENDING_SIGN_IN_WINDOW_MS - 1), (c, now)],
            focus_history: vec![(b, now - 5)],
        });
        assert_eq!(
            d,
            RouteDecision::Profile {
                id: b,
                reason: RouteReason::RecentFocus
            }
        );
    }

    #[test]
    fn single_running() {
        let (a, _, _) = ids();
        let d = decide(&RoutingInput {
            now: 0,
            running: vec![a],
            ..Default::default()
        });
        assert_eq!(
            d,
            RouteDecision::Profile {
                id: a,
                reason: RouteReason::OnlyRunning
            }
        );
    }

    #[test]
    fn latest_focus_wins() {
        let (a, b, _) = ids();
        let d = decide(&RoutingInput {
            now: 10_000,
            running: vec![a, b],
            focus_history: vec![(a, 1_000), (b, 2_000), (a, 3_000)],
            ..Default::default()
        });
        assert_eq!(
            d,
            RouteDecision::Profile {
                id: a,
                reason: RouteReason::RecentFocus
            }
        );
    }

    #[test]
    fn ask_when_ambiguous() {
        let (a, b, c) = ids();
        let now = 100 * 60 * 1000;
        let d = decide(&RoutingInput {
            now,
            running: vec![a, b, c],
            focus_history: vec![(c, 0), (b, 1)],
            ..Default::default()
        });
        assert_eq!(d, RouteDecision::Ask(vec![b, c, a]));
    }

    #[test]
    fn nothing_running_asks_with_empty_list() {
        assert_eq!(decide(&RoutingInput::default()), RouteDecision::Ask(vec![]));
    }
}
