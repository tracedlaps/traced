//! The phone's accelerometer through the Kotlin plugin (`android/imu`), in the GPS plugin's JSON envelope.

use chrono::DateTime;
use serde::Deserialize;
use traced_core::imu::ImuSample;

/// A drained sample: epoch ms, then acceleration and gravity (m/s², device axes).
#[derive(Debug, Deserialize)]
struct SampleDto(i64, [f32; 3], [f32; 3]);

impl From<SampleDto> for ImuSample {
    fn from(SampleDto(ms, accel, gravity): SampleDto) -> Self {
        ImuSample { timestamp: DateTime::from_timestamp_millis(ms).unwrap_or_default(), accel, gravity }
    }
}

#[cfg(target_os = "android")]
pub use android::AndroidImu;

#[cfg(target_os = "android")]
mod android {
    use std::{sync::Arc, time::Duration};

    use dioxus::logger::tracing::{info, warn};
    use futures::stream::BoxStream;
    use serde::Deserialize;
    use traced_core::imu::ImuSample;

    use super::SampleDto;
    use crate::gps::wire::invoke;

    const DRAIN_EVERY: Duration = Duration::from_millis(100);

    #[manganis::ffi("android/imu")]
    extern "Kotlin" {
        pub type ImuPlugin;
        pub fn start(this: &ImuPlugin) -> String;
        pub fn stop(this: &ImuPlugin, session: i32) -> String;
        pub fn drain(this: &ImuPlugin) -> String;
    }

    #[derive(Deserialize)]
    struct Started {
        session: i32,
        fused: bool,
    }

    /// App-lifetime handle to the plugin.
    pub struct AndroidImu(Arc<ImuPlugin>);

    impl AndroidImu {
        pub fn new() -> Option<Self> {
            crate::android::load_plugin("dev.traced.imu.ImuPlugin").map(|p| Self(Arc::new(ImuPlugin::from_global_ref(p))))
        }

        /// Samples until dropped; empty if the phone has no accelerometer.
        pub fn stream(&self) -> BoxStream<'static, ImuSample> {
            let plugin = self.0.clone();
            Box::pin(async_stream::stream! {
                let started = match invoke::<Started>(start(&plugin)) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("no accelerometer: {e}");
                        return;
                    }
                };
                info!("IMU started, fused: {}", started.fused);
                let _stop = Stop(plugin.clone(), started.session);
                loop {
                    tokio::time::sleep(DRAIN_EVERY).await;
                    match invoke::<Vec<SampleDto>>(drain(&plugin)) {
                        Ok(dtos) => for dto in dtos {
                            yield ImuSample::from(dto);
                        },
                        Err(e) => warn!("IMU drain: {e}"),
                    }
                }
            })
        }
    }

    /// Stops the plugin's session when the stream ends or is dropped.
    struct Stop(Arc<ImuPlugin>, i32);

    impl Drop for Stop {
        fn drop(&mut self) {
            if let Err(e) = invoke::<()>(stop(&self.0, self.1)) {
                warn!("IMU stop failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_samples() {
        let json = r#"[[1700000000123,[0.5,-1.0,0.0],[0.0,9.8,0.1]]]"#;
        let [dto]: [SampleDto; 1] = serde_json::from_str(json).unwrap();
        let s = ImuSample::from(dto);
        assert_eq!(s.timestamp.timestamp_millis(), 1_700_000_000_123);
        assert_eq!((s.accel, s.gravity), ([0.5, -1.0, 0.0], [0.0, 9.8, 0.1]));
    }
}
