//! Terminal UI modules — compiled only with the `tui` feature.

#[cfg(feature = "tui")]
pub mod dashboard;
#[cfg(feature = "tui")]
pub mod run_watch;
#[cfg(feature = "tui")]
pub mod status_watch;
#[cfg(feature = "tui")]
pub mod widgets;
