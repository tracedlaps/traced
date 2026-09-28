use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use dioxus::logger::tracing::{debug, info, warn};
use futures::stream::BoxStream;
use tokio::sync::{watch, Mutex as AsyncMutex};
use traced_core::gps::{FixSource, GPSFix, GpsError, GpsProvider, GpsStatus};

use super::wire::{invoke, FixDto, Permission, PermissionDto, StartOpts, StartedDto, StatusDto};

const DRAIN_EVERY: Duration = Duration::from_millis(100);
/// Drains between status refreshes (~1 s).
const STATUS_EVERY: u32 = 10;
/// A fix older than this drops the status back to Searching.
const FIX_STALE: Duration = Duration::from_secs(2);

// Path is relative to CARGO_MANIFEST_DIR with no leading slash: the macro both `concat!`s it
// (Gradle module for dx) and `Path::join`s it (namespace lookup), and a leading `/` breaks the join.
#[manganis::ffi("android/gps")]
extern "Kotlin" {
    pub type GpsPlugin;
    pub fn check_permissions(this: &GpsPlugin) -> String;
    /// Blocks up to 60 s on the permission dialog: call from `spawn_blocking`.
    pub fn request_permissions(this: &GpsPlugin) -> String;
    pub fn start(this: &GpsPlugin, opts_json: &str) -> String;
    pub fn stop(this: &GpsPlugin) -> String;
    pub fn drain_fixes(this: &GpsPlugin) -> String;
    pub fn status(this: &GpsPlugin) -> String;
    pub fn set_recording(this: &GpsPlugin, on: bool) -> String;
    pub fn open_app_settings(this: &GpsPlugin) -> String;
}

pub fn construct_plugin() -> Result<GpsPlugin, GpsError> {
    const CLASS: &str = "dev.traced.gps.GpsPlugin";
    crate::android::load_plugin(CLASS)
        .map(GpsPlugin::from_global_ref)
        .ok_or_else(|| GpsError::Unavailable(format!("failed to construct {CLASS}")))
}

/// The phone's GPS through the Kotlin plugin: Fused, or `LocationManager` as fallback.
pub struct AndroidGpsProvider {
    plugin: Arc<GpsPlugin>,
    status: watch::Sender<GpsStatus>,
    last_fix: Arc<Mutex<Option<GPSFix>>>,
    /// Held by a running stream, so a new one starts only once the last has stopped the plugin.
    session: Arc<AsyncMutex<()>>,
}

impl AndroidGpsProvider {
    pub fn new() -> Result<Self, GpsError> {
        Ok(Self {
            plugin: Arc::new(construct_plugin()?),
            status: watch::Sender::default(),
            last_fix: Arc::default(),
            session: Arc::default(),
        })
    }

    /// Shows the system dialog unless already granted (or blocked).
    pub async fn request_permission(&self) -> Result<Permission, GpsError> {
        let plugin = self.plugin.clone();
        let permission: Permission =
            blocking(move || invoke::<PermissionDto>(request_permissions(&plugin)))
                .await?
                .into();
        info!("location permission: {permission:?}");
        if permission == Permission::Granted && *self.status.borrow() == GpsStatus::Disconnected {
            self.status.send_replace(GpsStatus::Searching);
        }
        Ok(permission)
    }

    pub fn open_app_settings(&self) -> Result<(), GpsError> {
        invoke(open_app_settings(&self.plugin))
    }

    pub fn location_enabled(&self) -> Result<bool, GpsError> {
        invoke::<StatusDto>(status(&self.plugin)).map(|s| s.location_enabled)
    }

    /// Keeps the screen on and the recording alive with the phone locked (a foreground service).
    pub fn set_recording(&self, on: bool) -> Result<(), GpsError> {
        invoke(set_recording(&self.plugin, on))
    }

    /// `force_location_manager` (dev) skips Fused.
    pub fn stream(
        &self,
        force_location_manager: bool,
    ) -> BoxStream<'static, Result<GPSFix, GpsError>> {
        let (plugin, tx, last_fix, session) = (
            self.plugin.clone(),
            self.status.clone(),
            self.last_fix.clone(),
            self.session.clone(),
        );
        let opts = serde_json::to_string(&StartOpts {
            interval_ms: DRAIN_EVERY.as_millis() as u64,
            force_location_manager,
        })
        .unwrap();
        Box::pin(async_stream::stream! {
            let _session = session.lock_owned().await;
            // The guard comes back from the blocking task, so a stream dropped mid-start still stops.
            let started = tokio::task::spawn_blocking({
                let (plugin, tx) = (plugin.clone(), tx.clone());
                move || (Stop(plugin.clone(), tx), invoke::<StartedDto>(start(&plugin, &opts)))
            })
            .await;
            let (_stop, source) = match started {
                Ok((stop, Ok(s))) => (stop, FixSource::from(s.source)),
                Ok((_, Err(e))) => {
                    yield Err(e);
                    return;
                }
                Err(e) => {
                    yield Err(GpsError::Unavailable(e.to_string()));
                    return;
                }
            };
            info!("GPS started: {source:?}");
            let mut last_at = None::<Instant>;
            for tick in 0u32.. {
                tokio::time::sleep(DRAIN_EVERY).await;
                match invoke::<Vec<FixDto>>(drain_fixes(&plugin)) {
                    Ok(dtos) => for dto in dtos {
                        // Mock locations are kept in debug builds, so the emulator works.
                        if dto.is_mock && !cfg!(debug_assertions) {
                            continue;
                        }
                        let fix = GPSFix::from(dto);
                        debug!("fix {source:?}: {fix}");
                        (*last_fix.lock().unwrap(), last_at) = (Some(fix), Some(Instant::now()));
                        yield Ok(fix);
                    },
                    Err(e) => yield Err(e),
                }
                if tick % STATUS_EVERY == 0 {
                    match invoke::<StatusDto>(status(&plugin)) {
                        Ok(s) => {
                            let fresh = last_at.is_some_and(|t| t.elapsed() < FIX_STALE);
                            tx.send_replace(match *last_fix.lock().unwrap() {
                                Some(fix) if fresh && s.location_enabled => GpsStatus::Fixed {
                                    satellites: s.satellites_used,
                                    accuracy_m: fix.accuracy,
                                    source: s.source.map_or(source, Into::into),
                                },
                                _ => GpsStatus::Searching,
                            });
                        }
                        Err(e) => yield Err(e),
                    }
                }
            }
        })
    }
}

impl GpsProvider for AndroidGpsProvider {
    /// Checks (never requests) the location permission.
    fn init(&mut self) -> Result<(), GpsError> {
        if !invoke::<PermissionDto>(check_permissions(&self.plugin))?.granted {
            return Err(GpsError::PermissionDenied);
        }
        self.status.send_replace(GpsStatus::Searching);
        Ok(())
    }

    fn deinit(&self) {
        self.status.send_replace(GpsStatus::Disconnected);
    }

    fn stream_fixes(&self) -> BoxStream<'static, Result<GPSFix, GpsError>> {
        self.stream(false)
    }

    fn get_fix(&self) -> Option<GPSFix> {
        *self.last_fix.lock().unwrap()
    }

    fn status(&self) -> watch::Receiver<GpsStatus> {
        self.status.subscribe()
    }
}

/// Stops the plugin when a fix stream ends or is dropped.
struct Stop(Arc<GpsPlugin>, watch::Sender<GpsStatus>);

impl Drop for Stop {
    fn drop(&mut self) {
        if let Err(e) = invoke::<()>(stop(&self.0)) {
            warn!("GPS stop failed: {e}");
        }
        self.1.send_replace(GpsStatus::Searching);
    }
}

/// Runs a blocking plugin call off the async threads.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, GpsError> + Send + 'static,
) -> Result<T, GpsError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| GpsError::Unavailable(e.to_string()))?
}
