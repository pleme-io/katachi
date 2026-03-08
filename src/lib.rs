//! Katachi (形) — UI enhancement plugin for Neovim.
//!
//! Rust replacement for nui.nvim + noice.nvim + nvim-notify.
//! Provides toast-style notifications with auto-dismiss, floating
//! cmdline input, and custom message routing.
//!
//! Part of the blnvim-ng distribution — a Rust-native Neovim plugin suite.
//! Built with [`nvim-oxi`](https://github.com/noib3/nvim-oxi) for zero-cost
//! Neovim API bindings, [`tane`] SDK, and [`waku`] UI primitives.

pub mod message;
pub mod notify;
pub mod render;

use message::{Message, Severity};
use notify::NotifyQueue;
use nvim_oxi as oxi;
use std::cell::RefCell;
use std::time::Instant;
use tane::prelude::*;

thread_local! {
    static QUEUE: RefCell<NotifyQueue> = RefCell::new(NotifyQueue::new());
    static RENDERED: RefCell<Vec<render::RenderedNotification>> = RefCell::new(Vec::new());
}

/// Convert a `tane::Error` into an `oxi::Error` via the API error path.
fn tane_err(e: tane::Error) -> oxi::Error {
    oxi::Error::from(oxi::api::Error::Other(e.to_string()))
}

/// Push a notification into the queue and re-render.
fn do_notify(severity: Severity, content: &str) -> oxi::Result<()> {
    let now = Instant::now();
    QUEUE.with(|q| {
        q.borrow_mut().push(Message::new(severity, content), now);
    });
    refresh_display()?;
    Ok(())
}

/// Re-render all active notifications.
fn refresh_display() -> oxi::Result<()> {
    // Close existing windows.
    RENDERED.with(|r| {
        let mut rendered = r.borrow_mut();
        render::close_all(&mut rendered).ok();
    });

    // Tick to expire old notifications.
    let now = Instant::now();
    QUEUE.with(|q| {
        q.borrow_mut().tick(now);
    });

    // Render active notifications.
    QUEUE.with(|q| {
        let queue = q.borrow();
        let active = queue.active();
        if active.is_empty() {
            return;
        }
        match render::render_all(active) {
            Ok(new_rendered) => {
                RENDERED.with(|r| {
                    *r.borrow_mut() = new_rendered;
                });
            }
            Err(e) => {
                oxi::print!("katachi render error: {e}");
            }
        }
    });

    Ok(())
}

#[oxi::plugin]
fn katachi() -> oxi::Result<()> {
    // Set up highlight groups.
    render::setup_highlights().map_err(tane_err)?;

    // Register user commands.
    UserCommand::new("KatachiInfo")
        .one_arg()
        .desc("Show an info notification")
        .register(|args| {
            let text = args.args.unwrap_or_default();
            do_notify(Severity::Info, &text).ok();
            Ok(())
        })
        .map_err(tane_err)?;

    UserCommand::new("KatachiWarn")
        .one_arg()
        .desc("Show a warning notification")
        .register(|args| {
            let text = args.args.unwrap_or_default();
            do_notify(Severity::Warn, &text).ok();
            Ok(())
        })
        .map_err(tane_err)?;

    UserCommand::new("KatachiError")
        .one_arg()
        .desc("Show an error notification")
        .register(|args| {
            let text = args.args.unwrap_or_default();
            do_notify(Severity::Error, &text).ok();
            Ok(())
        })
        .map_err(tane_err)?;

    UserCommand::new("KatachiDismissAll")
        .desc("Dismiss all notifications")
        .register(|_args| {
            QUEUE.with(|q| q.borrow_mut().dismiss_all());
            RENDERED.with(|r| {
                let mut rendered = r.borrow_mut();
                render::close_all(&mut rendered).ok();
            });
            Ok(())
        })
        .map_err(tane_err)?;

    Ok(())
}
