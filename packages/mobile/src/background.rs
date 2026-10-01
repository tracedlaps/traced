//! Keeping a recording alive with the screen off, through the Kotlin plugin (`android/background`), in the GPS plugin's JSON envelope.

use serde::Deserialize;

/// Whether recording with the screen off needs the user's help in Settings.
#[derive(Debug, Clone, Copy, Deserialize)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))] // only Android fills it
pub struct Check {
    /// The phone's own battery manager ignores the system exemption (OnePlus, OPPO, realme).
    pub strict: bool,
    /// Exempt from battery optimisation: on a strict phone, only its "Allow background activity" grants it.
    pub exempt: bool,
    /// The last run was killed mid-recording.
    pub killed: bool,
    /// The recording's notification can show (always before Android 13).
    pub notifications: bool,
}

#[cfg(target_os = "android")]
pub use android::AndroidBackground;

#[cfg(target_os = "android")]
mod android {
    use traced_core::gps::GpsError;

    use super::Check;
    use crate::gps::wire::invoke;

    #[manganis::ffi("android/background")]
    extern "Kotlin" {
        pub type BackgroundPlugin;
        pub fn set_recording(this: &BackgroundPlugin, on: bool) -> String;
        pub fn check(this: &BackgroundPlugin) -> String;
        pub fn request_notifications(this: &BackgroundPlugin) -> String;
    }

    /// App-lifetime handle to the plugin.
    pub struct AndroidBackground(BackgroundPlugin);

    impl AndroidBackground {
        pub fn new() -> Option<Self> {
            crate::android::load_plugin("dev.traced.background.BackgroundPlugin").map(|p| Self(BackgroundPlugin::from_global_ref(p)))
        }

        /// Keeps the screen on and the recording alive with the phone locked (a foreground service).
        pub fn set_recording(&self, on: bool) -> Result<(), GpsError> {
            invoke(set_recording(&self.0, on))
        }

        pub fn check(&self) -> Result<Check, GpsError> {
            invoke(check(&self.0))
        }

        /// Shows the notification dialog, if needed; blocks until it's answered.
        pub fn request_notifications(&self) -> Result<(), GpsError> {
            invoke(request_notifications(&self.0))
        }
    }
}
