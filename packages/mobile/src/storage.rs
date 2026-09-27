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
    recorder::{FixConsumer, RecordingSummary},
    store::{SavedRecording, Store, StoreError},
    timing::Reference,
    track::{Track, TrackBuilder},
};

use crate::tracks::PRESETS;

#[cfg(target_os = "android")]
mod plugin {
    #[manganis::ffi("android/storage")]
    extern "Kotlin" {
        pub type StoragePlugin;
        pub fn data_dir(this: &StoragePlugin) -> String;
        /// Blocks until the user picks or cancels: call from `spawn_blocking`.
        pub fn pick_file(this: &StoragePlugin) -> String;
        /// Blocks until the user saves or cancels: call from `spawn_blocking`.
        pub fn save_file(this: &StoragePlugin, path: &str, name: &str) -> String;
    }

    pub fn construct() -> Option<StoragePlugin> {
        crate::android::load_plugin("dev.traced.storage.StoragePlugin")
            .map(StoragePlugin::from_global_ref)
    }

    /// Runs a blocking plugin call off the UI thread and unwraps its reply envelope.
    pub async fn call<T: serde::de::DeserializeOwned + Send + 'static>(
        f: impl FnOnce(&StoragePlugin) -> Result<String, String> + Send + 'static,
    ) -> Result<T, String> {
        #[derive(serde::Deserialize)]
        struct Reply {
            ok: bool,
            #[serde(default)]
            data: serde_json::Value,
            #[serde(default)]
            detail: String,
        }
        tokio::task::spawn_blocking(move || {
            let plugin = construct().ok_or("the storage plugin failed to load")?;
            let reply: Reply = serde_json::from_str(&f(&plugin)?).map_err(|e| e.to_string())?;
            if !reply.ok {
                return Err(reply.detail);
            }
            T::deserialize(reply.data).map_err(|e| e.to_string())
        })
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

/// Saves each fix as it arrives, so a crash loses at most the one being written.
struct Journal(Uuid);

impl FixConsumer for Journal {
    fn on_fix(&mut self, fix: GPSFix) {
        with_store("save fix", |s| s.append_fix(self.0, &fix));
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

/// The best lap on `track` so far, to race against.
pub fn reference(track: &Track) -> Option<Reference> {
    with_store("load best lap", |s| s.reference(track)).flatten()
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

/// Exports everything to a file the user chooses; `Ok(false)` if they cancel.
pub async fn export_backup() -> Result<bool, String> {
    let name = format!("traced-{}.db", chrono::Local::now().format("%Y-%m-%d"));
    let store = |path: PathBuf| {
        with_store("export", |s| s.export(&path)).ok_or("Couldn't export the data.".to_string())
    };

    #[cfg(target_os = "android")]
    {
        let path = data_dir().ok_or("no data dir")?.join("export.db");
        store(path.clone())?;
        let arg = path.to_string_lossy().into_owned();
        let saved = plugin::call(move |p| plugin::save_file(p, &arg, &name)).await;
        let _ = fs::remove_file(&path);
        saved
    }

    #[cfg(not(target_os = "android"))]
    match rfd::AsyncFileDialog::new()
        .set_file_name(&name)
        .save_file()
        .await
    {
        Some(file) => store(file.path().into()).map(|()| true),
        None => Ok(false),
    }
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
