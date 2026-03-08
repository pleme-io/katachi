//! Katachi (形) — UI component framework for Neovim: popups, inputs, menus, notifications, command palette
//!
//! Part of the blnvim-ng distribution — a Rust-native Neovim plugin suite.
//! Built with [`nvim-oxi`](https://github.com/noib3/nvim-oxi) for zero-cost
//! Neovim API bindings.

use nvim_oxi as oxi;

#[oxi::plugin]
fn katachi() -> oxi::Result<()> {
    Ok(())
}
