//! Where the app keeps its data, and the one store in it.

use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
};

use dioxus::logger::tracing::warn;
use serde::Deserialize;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use traced_core::{
    gps::GPSFix,
    imu::{Alignment, ImuSample},
    recorder::{FixConsumer, RecordingSummary},
    store::{SavedRecording, Store, StoreError},
    timing::{along_lap, best_lap, Reference},
    track::{Track, TrackBuilder},
};

use crate::tracks::{PRESETS, PRESETS_VERSION};

#[cfg(target_os = "android")]
pub(crate) mod plugin {
    #[manganis::ffi("android/storage")]
    extern "Kotlin" {
        pub type StoragePlugin;
        pub fn data_dir(this: &StoragePlugin) -> String;
        pub fn cache_dir(this: &StoragePlugin) -> String;
        /// `paths` newline-separated.
        pub fn share_files(this: &StoragePlugin, paths: &str, title: &str) -> String;
        pub fn take_incoming(this: &StoragePlugin) -> String;
        /// Blocks until the user picks or cancels: call from `spawn_blocking`.
        pub fn pick_file(this: &StoragePlugin) -> String;
        /// Blocks until the user saves or cancels: call from `spawn_blocking`.
        pub fn save_file(this: &StoragePlugin, path: &str, name: &str) -> String;
        /// Blocks on the network: call from `spawn_blocking`.
        pub fn fetch(this: &StoragePlugin, url: &str) -> String;
        /// Blocks on the download: call from `spawn_blocking`.
        pub fn install_update(this: &StoragePlugin, url: &str) -> String;
    }

    pub fn construct() -> Option<StoragePlugin> {
        crate::android::load_plugin("dev.traced.storage.StoragePlugin")
            .map(StoragePlugin::from_global_ref)
    }

    /// A call's error when the network is unreachable.
    pub const OFFLINE: &str = "offline";

    /// Makes a plugin call and unwraps its reply envelope.
    pub fn call_now<T: serde::de::DeserializeOwned>(
        f: impl FnOnce(&StoragePlugin) -> Result<String, String>,
    ) -> Result<T, String> {
        #[derive(serde::Deserialize)]
        struct Reply {
            ok: bool,
            #[serde(default)]
            data: serde_json::Value,
            #[serde(default)]
            error: String,
            #[serde(default)]
            detail: String,
        }
        let plugin = construct().ok_or("the storage plugin failed to load")?;
        let reply: Reply = serde_json::from_str(&f(&plugin)?).map_err(|e| e.to_string())?;
        if !reply.ok {
            return Err(if reply.error == OFFLINE { reply.error } else { reply.detail });
        }
        T::deserialize(reply.data).map_err(|e| e.to_string())
    }

    /// `call_now` off the UI thread, for calls that block.
    pub async fn call<T: serde::de::DeserializeOwned + Send + 'static>(
        f: impl FnOnce(&StoragePlugin) -> Result<String, String> + Send + 'static,
    ) -> Result<T, String> {
        tokio::task::spawn_blocking(move || call_now(f))
            .await
            .map_err(|e| e.to_string())?
    }
}

/// `None` if it couldn't be opened: the app then runs on the presets and saves nothing.
static STORE: LazyLock<Option<Mutex<Store>>> = LazyLock::new(|| {
    open()
        .inspect_err(|e| warn!("store unavailable: {e}"))
        .ok()
        .map(Mutex::new)
});

/// The app's private files dir on Android, the user's data dir (e.g. `~/.local/share/traced`) elsewhere.
fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    let dir = {
        plugin::data_dir(&plugin::construct()?)
            .ok()
            .map(PathBuf::from)
    };
    #[cfg(not(target_os = "android"))]
    let dir = dirs::data_dir().map(|d| d.join("traced"));
    dir
}

/// Opens the store, seeding the presets into an empty one.
fn open() -> Result<Store, Box<dyn Error>> {
    let dir = data_dir().ok_or("no data dir")?;
    fs::create_dir_all(&dir)?;
    let store = Store::open(dir.join("traced.db"))?;
    if store.tracks()?.is_empty() {
        PRESETS.iter().try_for_each(|p| store.put_track(&p.track))?;
    }
    // A preset an older release seeded takes this one's name and tags, unless its gates were moved.
    if store.setting("presets")?.as_deref() != Some(PRESETS_VERSION) {
        for p in PRESETS.iter() {
            if store.track(p.track.uuid())?.is_some_and(|t| t.same_geometry(&p.track)) {
                store.put_track(&p.track)?;
            }
        }
        store.put_setting("presets", PRESETS_VERSION)?;
    }
    if let Err(e) = store.recover() {
        warn!("recover recordings: {e}");
    }
    Ok(store)
}

/// Runs `f` on the store, logging a failure as `what`. `None` without a store or on failure.
fn with_store<T>(what: &str, f: impl FnOnce(&mut Store) -> Result<T, StoreError>) -> Option<T> {
    let mut store = STORE.as_ref()?.lock().unwrap();
    f(&mut store).inspect_err(|e| warn!("{what}: {e}")).ok()
}

/// The stored tracks, or the presets alone without a store.
pub fn load_tracks() -> Vec<Arc<Track>> {
    let tracks = with_store("load tracks", |s| s.tracks()).unwrap_or_else(|| {
        PRESETS
            .iter()
            .filter_map(|p| p.track.clone().build().ok())
            .collect()
    });
    tracks.into_iter().map(Arc::new).collect()
}

/// Saves `track` (new or edited). `None` if it couldn't be.
pub fn put_track(track: &TrackBuilder) -> Option<()> {
    with_store("save track", |s| s.put_track(track))
}

/// Saved track `id`'s spec, to edit.
pub fn load_spec(id: Uuid) -> Option<TrackBuilder> {
    with_store("load track", |s| s.track(id)).flatten()
}

/// Deletes track `id` and its recordings. `None` if it couldn't.
pub fn delete_track(id: Uuid) -> Option<()> {
    with_store("delete track", |s| s.delete_track(id))
}

/// Times recording `id` against `track` instead. `None` if it couldn't.
pub fn set_recording_track(id: Uuid, track: &Arc<Track>) -> Option<()> {
    with_store("change recording's track", |s| s.set_recording_track(id, track))
}

/// Deletes recording `id`, with its fixes and samples. `None` if it couldn't.
pub fn delete_recording(id: Uuid) -> Option<()> {
    with_store("delete recording", |s| s.delete_recording(id))
}

pub fn setting(key: &str) -> Option<String> {
    with_store("load setting", |s| s.setting(key)).flatten()
}

pub fn put_setting(key: &str, value: &str) -> Option<()> {
    with_store("save setting", |s| s.put_setting(key, value))
}

/// Saves each fix and sample as it arrives, so a crash loses at most the one being written.
struct Journal(Uuid);

impl FixConsumer for Journal {
    fn on_fix(&mut self, fix: GPSFix) {
        with_store("save fix", |s| s.append_fix(self.0, &fix));
    }

    fn on_sample(&mut self, sample: ImuSample) {
        with_store("save IMU sample", |s| s.append_sample(self.0, &sample));
    }
}

/// Starts keeping a recording: its id, and the journal its `Recorder` saves fixes through.
/// `None` without a store, when it isn't kept.
pub fn begin(
    started_at: DateTime<Utc>,
    track: Option<&Track>,
) -> Option<(Uuid, Box<dyn FixConsumer + Send>)> {
    let id = with_store("start recording", |s| {
        s.begin_recording(started_at, track.map(|t| t.uuid))
    })?;
    Some((id, Box::new(Journal(id))))
}

/// Stores how recording `id` went (its fixes are already in), or drops it without any.
pub fn finish(id: Uuid, summary: &RecordingSummary) {
    with_store("finish recording", |s| s.finish_recording(id, summary, false));
}

/// How the phone sat in its mount last time, so the g-meter can start aligned.
pub fn last_alignment() -> Option<Alignment> {
    with_store("load alignment", |s| s.last_alignment()).flatten()
}

/// The best lap on `track` so far, to race against.
pub fn reference(track: &Track) -> Option<Reference> {
    with_store("load best lap", |s| s.reference(track)).flatten()
}

/// The best lap on `track` so far, each fix with its distance past the start gate; empty before any.
pub fn best_lap_along(track: &Track) -> Vec<(f64, GPSFix)> {
    with_store("load best lap", |s| {
        let recordings = s.recordings()?;
        let Some((id, lap)) = best_lap(&recordings, track.uuid) else { return Ok(Vec::new()) };
        Ok(along_lap(track, lap, &s.fixes(id)?))
    })
    .unwrap_or_default()
}

/// Saved recording `id`'s fixes.
pub fn fixes(id: Uuid) -> Vec<GPSFix> {
    with_store("load fixes", |s| s.fixes(id)).unwrap_or_default()
}

/// Saved recording `id`'s fixes and samples, to replay; `None` without any fixes.
pub fn replay_data(id: Uuid) -> Option<(Vec<GPSFix>, Vec<ImuSample>)> {
    with_store("load recording", |s| Ok((s.fixes(id)?, s.samples(id)?))).filter(|(f, _)| !f.is_empty())
}

/// Saved recordings, newest first.
pub fn recordings() -> Vec<SavedRecording> {
    with_store("load recordings", |s| s.recordings()).unwrap_or_default()
}

/// A file the user picked; on Android a copy in the app's cache.
#[derive(Deserialize)]
pub struct PickedFile {
    pub name: String,
    pub path: PathBuf,
}

/// Asks the user for a file through the system picker; `Ok(None)` if they cancel.
pub async fn pick_file() -> Result<Option<PickedFile>, String> {
    #[cfg(target_os = "android")]
    return plugin::call(plugin::pick_file).await;

    #[cfg(not(target_os = "android"))]
    Ok(rfd::AsyncFileDialog::new()
        .pick_file()
        .await
        .map(|f| PickedFile {
            name: f.file_name(),
            path: f.path().into(),
        }))
}

/// A file another app opened or shared with Traced, once: on Android a copy in the app's cache.
pub fn take_incoming() -> Option<PickedFile> {
    #[cfg(target_os = "android")]
    return plugin::call_now(plugin::take_incoming)
        .inspect_err(|e| warn!("take incoming file: {e}"))
        .ok()
        .flatten();

    #[cfg(not(target_os = "android"))]
    None
}

/// An empty scratch dir to write files to share or save into. Emptied on each call, so a share
/// must get it once; on Android the only place `share_files` can share from.
pub fn share_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    let dir = plugin::cache_dir(&plugin::construct()?)
        .ok()
        .map(|d| PathBuf::from(d).join("share"));
    #[cfg(not(target_os = "android"))]
    let dir = dirs::cache_dir().map(|d| d.join("traced/share"));
    let dir = dir?;
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).inspect_err(|e| warn!("share dir: {e}")).ok()?;
    Some(dir)
}

/// Opens the system share sheet for `files` (in `share_dir`); returns once it's up.
/// `Err` off Android, where there's no share sheet.
pub async fn share_files(files: Vec<PathBuf>, title: String) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let paths = files.iter().map(|f| f.to_string_lossy()).collect::<Vec<_>>().join("\n");
        plugin::call(move |p| plugin::share_files(p, &paths, &title)).await
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (files, title);
        Err("sharing is only on Android".into())
    }
}

/// Asks where to save a copy of `path`, offering `name`; `Ok(false)` if they cancel.
pub async fn save_file(path: PathBuf, name: String) -> Result<bool, String> {
    #[cfg(target_os = "android")]
    {
        let path = path.to_string_lossy().into_owned();
        plugin::call(move |p| plugin::save_file(p, &path, &name)).await
    }

    #[cfg(not(target_os = "android"))]
    match rfd::AsyncFileDialog::new().set_file_name(&name).save_file().await {
        Some(to) => fs::copy(&path, to.path()).map(|_| true).map_err(|e| e.to_string()),
        None => Ok(false),
    }
}

/// Exports everything to a file the user chooses; `Ok(false)` if they cancel.
pub async fn export_backup() -> Result<bool, String> {
    let name = format!("traced-{}.db", chrono::Local::now().format("%Y-%m-%d"));
    let path = share_dir().ok_or("no share dir")?.join(&name);
    with_store("export", |s| s.export(&path)).ok_or("export failed")?;
    save_file(path, name).await
}

/// Replaces everything with a backup the user picks; `Ok(false)` if they cancel.
pub async fn restore_backup() -> Result<bool, String> {
    let Some(file) = pick_file().await? else {
        return Ok(false);
    };
    let mut store = STORE
        .as_ref()
        .ok_or("no store to restore into")?
        .lock()
        .unwrap();
    store
        .restore(&file.path)
        .map_err(|e| format!("{}: {e}", file.name))?;
    Ok(true)
}
