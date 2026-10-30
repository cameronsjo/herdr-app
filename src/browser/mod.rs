//! Browser panes: a Chromium page in a block (`{ "browser": "b1", "url": … }`),
//! as a pane in a tab or a block in the shown layout.
//!
//! macOS builds with the `browser` feature run CEF in windowed mode: each page
//! is a native child view over GPUI's Metal view, kept on its block's bounds
//! every frame (native.rs, view.rs, runtime.rs). Every other build has the
//! same API with no browser behind it: blocks show a placeholder and the
//! commands say what's missing (off.rs). The window only talks to `Browsers`.

#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
pub mod keys;
#[cfg(any(test, all(target_os = "macos", feature = "browser")))]
pub mod registry;

#[cfg(all(target_os = "macos", feature = "browser"))]
mod native;
#[cfg(all(target_os = "macos", feature = "browser"))]
mod runtime;
#[cfg(all(target_os = "macos", feature = "browser"))]
mod view;
#[cfg(all(target_os = "macos", feature = "browser"))]
pub use native::{Browsers, Event};

#[cfg(not(all(target_os = "macos", feature = "browser")))]
mod off;
#[cfg(not(all(target_os = "macos", feature = "browser")))]
pub use off::{Browsers, Event};

/// What the window does after a browser reported something.
#[derive(Debug, Default)]
pub struct Update {
    pub browser: String,
    /// The page moved here: the block's saved url follows.
    pub url: Option<String>,
    /// The page took the keyboard (the person clicked it): its pane is the focused one now.
    pub focused: bool,
    /// The page asked to close (`window.close()`): its pane goes.
    pub close: bool,
}

/// Whether browser panes work in this build.
pub const AVAILABLE: bool = cfg!(all(target_os = "macos", feature = "browser"));
