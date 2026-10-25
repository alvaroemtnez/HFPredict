//! HFPredict's shared core.
//!
//! Everything the Android and GNOME apps have in common lives here, so the
//! two can only differ in how they draw: forecasts and maps from the
//! propagation engine (`hfcast`), the empirical correction HFcast fitted to
//! WSPR data, places and grid squares, the optional online readings, the
//! saved settings, and the map renderer.
//!
//! Much of the logic is ported from the HFcast app
//! (<https://github.com/jonathanmcsweet/hfcast>, Apache-2.0); see NOTICE.

#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!();

pub mod antenna;
pub mod app;
pub mod bands;
pub mod cache;
pub mod correct;
pub mod coverage;
pub mod engine;
pub mod exports;
pub mod forecast;
pub mod geo;
pub mod localtime;
pub mod maps;
pub mod modes;
pub mod online;
pub mod parallel;
pub mod places;
pub mod projection;
pub mod quality;
pub mod render;
pub mod ssn;
pub mod station;
pub mod store;

/// Every way a core call can fail (Kotlin: `CoreException`).
#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "ffi", derive(uniffi::Error))]
#[cfg_attr(feature = "ffi", uniffi(flat_error))]
pub enum CoreError {
    /// The engine refused a request or failed; its own message.
    #[error("engine: {0}")]
    Engine(String),
    /// A file under the app's directories could not be read or written.
    #[error("storage: {0}")]
    Storage(String),
    /// An online source could not be reached or answered nonsense.
    #[error("network: {0}")]
    Network(String),
    /// The network is switched off in Preferences.
    #[error("network use is switched off")]
    Offline,
}

pub use CoreError as Error;

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::Storage(e.to_string())
    }
}

pub use app::Core;
