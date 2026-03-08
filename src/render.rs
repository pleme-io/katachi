//! Render notifications as floating windows using waku primitives.
//!
//! Each active notification gets its own floating window, stacked
//! vertically from the top-right corner of the editor.

use crate::message::Severity;
use crate::notify::ActiveNotification;
use tane::highlight::Highlight;
use waku::border::BorderStyle;
use waku::float::FloatWindow;
use waku::layout::{Anchor, FloatLayout, Size};

/// Gap between stacked notification windows (rows).
const STACK_GAP: u32 = 1;

/// Default notification width in columns.
const NOTIFY_WIDTH: u32 = 40;

/// Right margin from the editor edge.
const RIGHT_MARGIN: i32 = 2;

/// Top margin from the editor edge.
const TOP_MARGIN: i32 = 1;

/// A rendered notification window.
pub struct RenderedNotification {
    /// The underlying float window.
    pub window: FloatWindow,
    /// The message ID this window represents.
    pub message_id: u64,
}

/// Register katachi highlight groups for notification severities.
pub fn setup_highlights() -> tane::Result<()> {
    Highlight::new(Severity::Info.highlight_group())
        .fg("#7dcfff")
        .bg("#1a1b26")
        .apply()?;

    Highlight::new(Severity::Warn.highlight_group())
        .fg("#e0af68")
        .bg("#1a1b26")
        .bold()
        .apply()?;

    Highlight::new(Severity::Error.highlight_group())
        .fg("#f7768e")
        .bg("#1a1b26")
        .bold()
        .apply()?;

    Ok(())
}

/// Build the layout for the Nth notification in the stack (0-indexed).
#[must_use]
fn notification_layout(index: usize, height: u32) -> FloatLayout {
    // Compute the row offset: each prior notification's height + gap.
    // For simplicity, we use a fixed per-slot offset since we don't
    // know the heights of prior notifications here. The caller should
    // accumulate the offset.
    #[allow(clippy::cast_possible_wrap)]
    let row_offset = TOP_MARGIN + (index as i32) * (height as i32 + STACK_GAP as i32);

    FloatLayout {
        width: Size::Fixed(NOTIFY_WIDTH),
        height: Size::Fixed(height),
        anchor: Anchor::NorthEast,
        row_offset,
        col_offset: -RIGHT_MARGIN,
    }
}

/// Render a single notification as a floating window.
///
/// The `stack_index` determines vertical position (0 = top of stack).
pub fn render_notification(
    notification: &ActiveNotification,
    stack_index: usize,
) -> tane::Result<RenderedNotification> {
    let lines = notification.message.render_lines();
    #[allow(clippy::cast_possible_truncation)]
    let height = (lines.len() as u32).max(1);
    let layout = notification_layout(stack_index, height);
    let border = border_for_severity(notification.message.severity);

    let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();

    let mut window = FloatWindow::new()
        .layout(layout)
        .border(border)
        .focusable(false);
    window.open()?;
    window.set_lines(&line_refs)?;

    Ok(RenderedNotification {
        window,
        message_id: notification.message.id,
    })
}

/// Render all active notifications as a vertical stack.
pub fn render_all(
    notifications: &[ActiveNotification],
) -> tane::Result<Vec<RenderedNotification>> {
    let mut rendered = Vec::with_capacity(notifications.len());
    for (i, notification) in notifications.iter().enumerate() {
        rendered.push(render_notification(notification, i)?);
    }
    Ok(rendered)
}

/// Close all rendered notification windows.
pub fn close_all(rendered: &mut Vec<RenderedNotification>) -> tane::Result<()> {
    for r in rendered.drain(..) {
        let mut win = r.window;
        win.close()?;
    }
    Ok(())
}

/// Choose border style based on severity.
#[must_use]
fn border_for_severity(severity: Severity) -> BorderStyle {
    match severity {
        Severity::Info => BorderStyle::Rounded,
        Severity::Warn => BorderStyle::Single,
        Severity::Error => BorderStyle::Double,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_layout_stacks_vertically() {
        let l0 = notification_layout(0, 2);
        let l1 = notification_layout(1, 2);
        let l2 = notification_layout(2, 2);

        // Each should be further down.
        assert!(l1.row_offset > l0.row_offset);
        assert!(l2.row_offset > l1.row_offset);
    }

    #[test]
    fn notification_layout_dimensions() {
        let layout = notification_layout(0, 3);
        match layout.width {
            Size::Fixed(w) => assert_eq!(w, NOTIFY_WIDTH),
            _ => panic!("expected fixed width"),
        }
        match layout.height {
            Size::Fixed(h) => assert_eq!(h, 3),
            _ => panic!("expected fixed height"),
        }
    }

    #[test]
    fn border_style_by_severity() {
        assert!(matches!(
            border_for_severity(Severity::Info),
            BorderStyle::Rounded
        ));
        assert!(matches!(
            border_for_severity(Severity::Warn),
            BorderStyle::Single
        ));
        assert!(matches!(
            border_for_severity(Severity::Error),
            BorderStyle::Double
        ));
    }
}
