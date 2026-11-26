#[cfg(windows)]
pub mod card;
#[cfg(windows)]
pub mod identity;
#[cfg(windows)]
pub mod install;
#[cfg(windows)]
pub mod language;
#[cfg(all(windows, feature = "setup"))]
pub mod prerequisites;
