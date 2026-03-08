//! Notification queue and lifecycle management.
//!
//! Pure Rust state machine for managing toast-style notifications.
//! Tracks a queue of pending messages, manages display slots,
//! and handles auto-dismiss timers based on message severity.

use crate::message::{Message, MessageSource, Severity};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Maximum number of notifications visible at once.
const DEFAULT_MAX_VISIBLE: usize = 5;

/// A notification that is currently being displayed.
#[derive(Debug)]
pub struct ActiveNotification {
    /// The message being displayed.
    pub message: Message,
    /// When this notification was first shown.
    pub shown_at: Instant,
    /// When this notification should auto-dismiss.
    pub dismiss_at: Instant,
    /// Whether the user has pinned this notification (prevents auto-dismiss).
    pub pinned: bool,
}

impl ActiveNotification {
    /// Create a new active notification from a message.
    fn new(message: Message, now: Instant) -> Self {
        let duration = message.effective_duration();
        Self {
            dismiss_at: now + duration,
            shown_at: now,
            message,
            pinned: false,
        }
    }

    /// Check if this notification has expired at the given time.
    #[must_use]
    pub fn is_expired(&self, now: Instant) -> bool {
        !self.pinned && now >= self.dismiss_at
    }

    /// Remaining time before auto-dismiss.
    #[must_use]
    pub fn remaining(&self, now: Instant) -> Duration {
        if self.pinned {
            Duration::MAX
        } else {
            self.dismiss_at.saturating_duration_since(now)
        }
    }

    /// Pin this notification to prevent auto-dismiss.
    pub fn pin(&mut self) {
        self.pinned = true;
    }

    /// Unpin and reset the dismiss timer from now.
    pub fn unpin(&mut self, now: Instant) {
        self.pinned = false;
        self.dismiss_at = now + self.message.effective_duration();
    }
}

/// Rule for routing messages from a particular source.
#[derive(Debug, Clone)]
pub struct RouteRule {
    /// Source pattern to match.
    pub source: MessageSource,
    /// Override the severity for matched messages.
    pub override_severity: Option<Severity>,
    /// Override the display duration.
    pub override_duration: Option<Duration>,
    /// If true, suppress (don't display) messages from this source.
    pub suppress: bool,
}

impl RouteRule {
    /// Create a route rule for the given source.
    #[must_use]
    pub fn new(source: MessageSource) -> Self {
        Self {
            source,
            override_severity: None,
            override_duration: None,
            suppress: false,
        }
    }

    /// Suppress messages from this source entirely.
    #[must_use]
    pub fn suppress(mut self) -> Self {
        self.suppress = true;
        self
    }

    /// Override the severity for matched messages.
    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.override_severity = Some(severity);
        self
    }

    /// Override the display duration for matched messages.
    #[must_use]
    pub fn duration(mut self, duration: Duration) -> Self {
        self.override_duration = Some(duration);
        self
    }

    /// Apply this rule to a message, returning `None` if suppressed.
    fn apply(&self, mut message: Message) -> Option<Message> {
        if self.suppress {
            return None;
        }
        if let Some(severity) = self.override_severity {
            message.severity = severity;
        }
        if let Some(duration) = self.override_duration {
            message.duration = Some(duration);
        }
        Some(message)
    }
}

/// The notification state machine.
///
/// Manages a queue of pending notifications and a set of active (visible)
/// notification slots. Call [`NotifyQueue::tick`] periodically to expire
/// old notifications and promote queued ones.
#[derive(Debug)]
pub struct NotifyQueue {
    /// Notifications currently visible on screen.
    active: Vec<ActiveNotification>,
    /// Pending notifications waiting for a display slot.
    pending: VecDeque<Message>,
    /// Maximum number of simultaneously visible notifications.
    max_visible: usize,
    /// Routing rules applied to incoming messages.
    routes: Vec<RouteRule>,
    /// Total number of messages ever pushed (for stats).
    total_pushed: u64,
    /// Total number of messages dismissed.
    total_dismissed: u64,
}

impl NotifyQueue {
    /// Create a new notification queue with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: Vec::new(),
            pending: VecDeque::new(),
            max_visible: DEFAULT_MAX_VISIBLE,
            routes: Vec::new(),
            total_pushed: 0,
            total_dismissed: 0,
        }
    }

    /// Set the maximum number of simultaneously visible notifications.
    #[must_use]
    pub fn max_visible(mut self, max: usize) -> Self {
        self.max_visible = max.max(1);
        self
    }

    /// Add a routing rule.
    pub fn add_route(&mut self, rule: RouteRule) {
        self.routes.push(rule);
    }

    /// Push a new message into the notification system.
    ///
    /// The message is routed through any matching rules, then either
    /// immediately displayed (if slots are available) or queued.
    ///
    /// Returns the message ID if the message was accepted (not suppressed).
    pub fn push(&mut self, message: Message, now: Instant) -> Option<u64> {
        let message = self.apply_routes(message)?;
        let id = message.id;
        self.total_pushed += 1;

        if self.active.len() < self.max_visible {
            self.active.push(ActiveNotification::new(message, now));
        } else {
            self.pending.push_back(message);
        }

        Some(id)
    }

    /// Tick the notification system: expire old notifications, promote pending ones.
    ///
    /// Returns the IDs of notifications that were dismissed this tick.
    pub fn tick(&mut self, now: Instant) -> Vec<u64> {
        let mut dismissed = Vec::new();

        // Remove expired active notifications.
        self.active.retain(|n| {
            if n.is_expired(now) {
                dismissed.push(n.message.id);
                false
            } else {
                true
            }
        });

        self.total_dismissed += dismissed.len() as u64;

        // Promote pending notifications to fill empty slots.
        while self.active.len() < self.max_visible {
            if let Some(message) = self.pending.pop_front() {
                self.active.push(ActiveNotification::new(message, now));
            } else {
                break;
            }
        }

        dismissed
    }

    /// Dismiss a specific notification by ID.
    ///
    /// Returns `true` if the notification was found and dismissed.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let len_before = self.active.len();
        self.active.retain(|n| n.message.id != id);
        let removed = self.active.len() < len_before;
        if removed {
            self.total_dismissed += 1;
        }

        // Also remove from pending queue.
        if !removed {
            let plen = self.pending.len();
            self.pending.retain(|m| m.id != id);
            return self.pending.len() < plen;
        }

        removed
    }

    /// Dismiss all notifications (active and pending).
    pub fn dismiss_all(&mut self) {
        self.total_dismissed += self.active.len() as u64;
        self.active.clear();
        self.pending.clear();
    }

    /// Pin a notification by ID (prevents auto-dismiss).
    pub fn pin(&mut self, id: u64) -> bool {
        if let Some(n) = self.active.iter_mut().find(|n| n.message.id == id) {
            n.pin();
            true
        } else {
            false
        }
    }

    /// Unpin a notification by ID (resets the dismiss timer).
    pub fn unpin(&mut self, id: u64, now: Instant) -> bool {
        if let Some(n) = self.active.iter_mut().find(|n| n.message.id == id) {
            n.unpin(now);
            true
        } else {
            false
        }
    }

    /// Get the currently active (visible) notifications.
    #[must_use]
    pub fn active(&self) -> &[ActiveNotification] {
        &self.active
    }

    /// Number of pending (queued) notifications.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Total number of messages ever pushed.
    #[must_use]
    pub fn total_pushed(&self) -> u64 {
        self.total_pushed
    }

    /// Total number of messages dismissed.
    #[must_use]
    pub fn total_dismissed(&self) -> u64 {
        self.total_dismissed
    }

    /// Compute the time until the next notification expires.
    ///
    /// Returns `None` if there are no active unpinned notifications.
    #[must_use]
    pub fn next_expiry(&self, now: Instant) -> Option<Duration> {
        self.active
            .iter()
            .filter(|n| !n.pinned)
            .map(|n| n.remaining(now))
            .min()
    }

    /// Apply routing rules to a message.
    fn apply_routes(&self, message: Message) -> Option<Message> {
        let mut msg = message;
        for rule in &self.routes {
            if rule.source == msg.source {
                msg = rule.apply(msg)?;
            }
        }
        Some(msg)
    }
}

impl Default for NotifyQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Instant {
        Instant::now()
    }

    #[test]
    fn push_and_active() {
        let mut q = NotifyQueue::new();
        let t = now();
        let id = q.push(Message::info("hello"), t);
        assert!(id.is_some());
        assert_eq!(q.active().len(), 1);
        assert_eq!(q.pending_count(), 0);
    }

    #[test]
    fn respects_max_visible() {
        let mut q = NotifyQueue::new().max_visible(2);
        let t = now();
        q.push(Message::info("one"), t);
        q.push(Message::info("two"), t);
        q.push(Message::info("three"), t);
        assert_eq!(q.active().len(), 2);
        assert_eq!(q.pending_count(), 1);
    }

    #[test]
    fn tick_expires_old_notifications() {
        let mut q = NotifyQueue::new();
        let t = now();
        q.push(Message::info("short").duration(Duration::from_millis(100)), t);

        // Not expired yet.
        let dismissed = q.tick(t + Duration::from_millis(50));
        assert!(dismissed.is_empty());
        assert_eq!(q.active().len(), 1);

        // Now expired.
        let dismissed = q.tick(t + Duration::from_millis(200));
        assert_eq!(dismissed.len(), 1);
        assert_eq!(q.active().len(), 0);
    }

    #[test]
    fn tick_promotes_pending() {
        let mut q = NotifyQueue::new().max_visible(1);
        let t = now();
        let id1 = q.push(Message::info("first").duration(Duration::from_millis(100)), t).unwrap();
        q.push(Message::info("second"), t);

        assert_eq!(q.active().len(), 1);
        assert_eq!(q.active()[0].message.id, id1);
        assert_eq!(q.pending_count(), 1);

        // Expire the first, promote the second.
        let dismissed = q.tick(t + Duration::from_millis(200));
        assert_eq!(dismissed, vec![id1]);
        assert_eq!(q.active().len(), 1);
        assert_eq!(q.pending_count(), 0);
        assert_ne!(q.active()[0].message.id, id1);
    }

    #[test]
    fn dismiss_by_id() {
        let mut q = NotifyQueue::new();
        let t = now();
        let id = q.push(Message::info("bye"), t).unwrap();
        assert!(q.dismiss(id));
        assert_eq!(q.active().len(), 0);
    }

    #[test]
    fn dismiss_nonexistent_returns_false() {
        let mut q = NotifyQueue::new();
        assert!(!q.dismiss(9999));
    }

    #[test]
    fn dismiss_from_pending() {
        let mut q = NotifyQueue::new().max_visible(1);
        let t = now();
        q.push(Message::info("active"), t);
        let pending_id = q.push(Message::info("pending"), t).unwrap();
        assert_eq!(q.pending_count(), 1);
        assert!(q.dismiss(pending_id));
        assert_eq!(q.pending_count(), 0);
    }

    #[test]
    fn dismiss_all_clears_everything() {
        let mut q = NotifyQueue::new().max_visible(2);
        let t = now();
        q.push(Message::info("a"), t);
        q.push(Message::info("b"), t);
        q.push(Message::info("c"), t);
        q.dismiss_all();
        assert_eq!(q.active().len(), 0);
        assert_eq!(q.pending_count(), 0);
    }

    #[test]
    fn pin_prevents_expiry() {
        let mut q = NotifyQueue::new();
        let t = now();
        let id = q.push(Message::info("sticky").duration(Duration::from_millis(100)), t).unwrap();
        assert!(q.pin(id));

        // Should not expire even after the duration.
        let dismissed = q.tick(t + Duration::from_secs(10));
        assert!(dismissed.is_empty());
        assert_eq!(q.active().len(), 1);
    }

    #[test]
    fn unpin_resets_timer() {
        let mut q = NotifyQueue::new();
        let t = now();
        let id = q.push(Message::info("sticky").duration(Duration::from_millis(100)), t).unwrap();
        q.pin(id);

        let t2 = t + Duration::from_secs(10);
        assert!(q.unpin(id, t2));

        // Should not be expired yet (timer just reset).
        let dismissed = q.tick(t2 + Duration::from_millis(50));
        assert!(dismissed.is_empty());

        // Now it should expire.
        let dismissed = q.tick(t2 + Duration::from_millis(200));
        assert_eq!(dismissed.len(), 1);
    }

    #[test]
    fn route_rule_suppress() {
        let mut q = NotifyQueue::new();
        q.add_route(RouteRule::new(MessageSource::LspProgress).suppress());

        let t = now();
        let msg = Message::info("progress").source(MessageSource::LspProgress);
        let id = q.push(msg, t);
        assert!(id.is_none());
        assert_eq!(q.active().len(), 0);
    }

    #[test]
    fn route_rule_override_severity() {
        let mut q = NotifyQueue::new();
        q.add_route(RouteRule::new(MessageSource::SearchCount).severity(Severity::Info));

        let t = now();
        let msg = Message::warn("search result").source(MessageSource::SearchCount);
        q.push(msg, t);

        assert_eq!(q.active().len(), 1);
        assert_eq!(q.active()[0].message.severity, Severity::Info);
    }

    #[test]
    fn route_rule_override_duration() {
        let mut q = NotifyQueue::new();
        q.add_route(
            RouteRule::new(MessageSource::LspProgress)
                .duration(Duration::from_millis(500)),
        );

        let t = now();
        let msg = Message::info("progress").source(MessageSource::LspProgress);
        q.push(msg, t);

        assert_eq!(q.active().len(), 1);
        assert_eq!(
            q.active()[0].message.effective_duration(),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn next_expiry_returns_soonest() {
        let mut q = NotifyQueue::new();
        let t = now();
        q.push(Message::info("short").duration(Duration::from_millis(100)), t);
        q.push(Message::info("long").duration(Duration::from_millis(1000)), t);

        let next = q.next_expiry(t).unwrap();
        assert!(next <= Duration::from_millis(100));
    }

    #[test]
    fn next_expiry_none_when_empty() {
        let q = NotifyQueue::new();
        assert!(q.next_expiry(now()).is_none());
    }

    #[test]
    fn next_expiry_none_when_all_pinned() {
        let mut q = NotifyQueue::new();
        let t = now();
        let id = q.push(Message::info("pinned"), t).unwrap();
        q.pin(id);
        assert!(q.next_expiry(t).is_none());
    }

    #[test]
    fn total_stats_tracking() {
        let mut q = NotifyQueue::new();
        let t = now();
        q.push(Message::info("a").duration(Duration::from_millis(10)), t);
        q.push(Message::info("b").duration(Duration::from_millis(10)), t);
        assert_eq!(q.total_pushed(), 2);

        q.tick(t + Duration::from_millis(100));
        assert_eq!(q.total_dismissed(), 2);
    }

    #[test]
    fn max_visible_at_least_one() {
        let q = NotifyQueue::new().max_visible(0);
        let t = now();
        let mut q = q;
        q.push(Message::info("should show"), t);
        assert_eq!(q.active().len(), 1);
    }

    #[test]
    fn mixed_severity_expiry_order() {
        let mut q = NotifyQueue::new();
        let t = now();

        // Info (3s), Warn (5s), Error (8s) — use defaults.
        q.push(Message::info("info msg"), t);
        q.push(Message::warn("warn msg"), t);
        q.push(Message::error("error msg"), t);

        // After 4s, only info should expire.
        let dismissed = q.tick(t + Duration::from_secs(4));
        assert_eq!(dismissed.len(), 1);
        assert_eq!(q.active().len(), 2);

        // After 6s, warn also expires.
        let dismissed = q.tick(t + Duration::from_secs(6));
        assert_eq!(dismissed.len(), 1);
        assert_eq!(q.active().len(), 1);

        // After 9s, error expires.
        let dismissed = q.tick(t + Duration::from_secs(9));
        assert_eq!(dismissed.len(), 1);
        assert_eq!(q.active().len(), 0);
    }

    #[test]
    fn fifo_ordering_in_queue() {
        let mut q = NotifyQueue::new().max_visible(1);
        let t = now();

        q.push(Message::info("first").duration(Duration::from_millis(100)), t);
        q.push(Message::info("second").duration(Duration::from_millis(100)), t);
        q.push(Message::info("third").duration(Duration::from_millis(100)), t);

        assert_eq!(q.active()[0].message.content, "first");

        q.tick(t + Duration::from_millis(200));
        assert_eq!(q.active()[0].message.content, "second");

        q.tick(t + Duration::from_millis(400));
        assert_eq!(q.active()[0].message.content, "third");
    }

    #[test]
    fn remaining_duration() {
        let t = now();
        let n = ActiveNotification::new(
            Message::info("test").duration(Duration::from_secs(5)),
            t,
        );
        let remaining = n.remaining(t + Duration::from_secs(2));
        assert!(remaining <= Duration::from_secs(3));
        assert!(remaining >= Duration::from_millis(2900));
    }

    #[test]
    fn remaining_pinned_is_max() {
        let t = now();
        let mut n = ActiveNotification::new(Message::info("test"), t);
        n.pin();
        assert_eq!(n.remaining(t + Duration::from_secs(100)), Duration::MAX);
    }

    #[test]
    fn multiple_routes_compose() {
        let mut q = NotifyQueue::new();
        // Two rules for the same source: first overrides severity, second overrides duration.
        q.add_route(RouteRule::new(MessageSource::LspProgress).severity(Severity::Warn));
        q.add_route(
            RouteRule::new(MessageSource::LspProgress)
                .duration(Duration::from_millis(200)),
        );

        let t = now();
        let msg = Message::info("lsp").source(MessageSource::LspProgress);
        q.push(msg, t);

        assert_eq!(q.active()[0].message.severity, Severity::Warn);
        assert_eq!(
            q.active()[0].message.effective_duration(),
            Duration::from_millis(200)
        );
    }
}
