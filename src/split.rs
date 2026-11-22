//! Split view: two sessions side by side, one of them active.
//!
//! Pure logic (which pair is shown as the active session changes) so it is
//! tested without GPUI.

use crate::tabs::SessionId;

/// Sessions shown side by side: `(left, right)`.
pub type Pair = (SessionId, SessionId);

/// Session to show next to `active` when splitting: the one before it in
/// tab order, or the one after it if it is first. `None` with one session.
pub fn partner(order: &[SessionId], active: SessionId) -> Option<SessionId> {
    let index = order.iter().position(|&id| id == active)?;
    match index {
        0 => order.get(1).copied(),
        i => Some(order[i - 1]),
    }
}

/// The pair after the active session changed from `prev` to `active`.
///
/// - Switching between the two panes keeps the pair.
/// - Activating another session puts it in the pane that was active.
/// - The split ends when a pane's session is gone, nothing is active, or
///   the change cannot be placed (no previous pane to replace).
pub fn follow(
    pair: Option<Pair>,
    prev: Option<SessionId>,
    active: Option<SessionId>,
    exists: impl Fn(SessionId) -> bool,
) -> Option<Pair> {
    let (left, right) = pair?;
    let active = active?;
    if !exists(left) || !exists(right) || left == right {
        return None;
    }
    if active == left || active == right {
        return Some((left, right));
    }
    let next = match prev {
        Some(p) if p == left => (active, right),
        Some(p) if p == right => (left, active),
        _ => return None,
    };
    (next.0 != next.1).then_some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partner_is_the_previous_tab() {
        assert_eq!(partner(&[1, 2, 3], 3), Some(2));
        assert_eq!(partner(&[1, 2, 3], 1), Some(2));
        assert_eq!(partner(&[1], 1), None);
        assert_eq!(partner(&[1, 2], 9), None);
    }

    #[test]
    fn follow_keeps_replaces_or_ends() {
        let all = |_| true;
        // Moving between panes keeps the pair.
        assert_eq!(follow(Some((1, 2)), Some(1), Some(2), all), Some((1, 2)));
        // Another session replaces the pane that was active.
        assert_eq!(follow(Some((1, 2)), Some(1), Some(3), all), Some((3, 2)));
        assert_eq!(follow(Some((1, 2)), Some(2), Some(3), all), Some((1, 3)));
        // No pane to replace, nothing active, a pane gone: no split.
        assert_eq!(follow(Some((1, 2)), Some(5), Some(3), all), None);
        assert_eq!(follow(Some((1, 2)), Some(1), None, all), None);
        assert_eq!(follow(Some((1, 2)), Some(1), Some(1), |id| id != 2), None);
        assert_eq!(follow(None, Some(1), Some(2), all), None);
        assert_eq!(follow(Some((1, 1)), Some(1), Some(1), all), None);
    }
}
