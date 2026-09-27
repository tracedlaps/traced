//! Where the app keeps its data, and the one store in it.

use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
};

use dioxus::logger::tracing::warn;
use serde::Deserialize;
use traced_core::{
    recorder::RecordingSummary,
    store::{SavedRecording, Store, StoreError},
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
    }

    pub fn construct() -> Option<StoragePlugin> {
        crate::android::load_plugin("dev.traced.storage.StoragePlugin")
            .map(StoragePlugin::from_global_ref)
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

/// Keeps a finished recording, unless it never got a fix. Blocks on the write.
pub fn save(summary: &RecordingSummary) {
    if !summary.fixes.is_empty() {
        with_store("save recording", |s| s.put_recording(summary));
    }
}

/// Saved recordings, newest first.
pub fn recordings() -> Vec<SavedRecording> {
    with_store("load recordings", |s| s.recordings()).unwrap_or_default()
}

/// A file the user picked, read whole.
#[derive(Deserialize)]
pub struct PickedFile {
    pub name: String,
    pub text: String,
}

/// Asks the user for a file through the system picker; `Ok(None)` if they cancel.
pub async fn pick_file() -> Result<Option<PickedFile>, String> {
    #[cfg(target_os = "android")]
    return tokio::task::spawn_blocking(|| {
        #[derive(Deserialize)]
        struct Reply {
            ok: bool,
            data: Option<PickedFile>,
            #[serde(default)]
            detail: String,
        }
        let plugin = plugin::construct().ok_or("the storage plugin failed to load")?;
        let reply: Reply =
            serde_json::from_str(&plugin::pick_file(&plugin)?).map_err(|e| e.to_string())?;
        reply.ok.then_some(reply.data).ok_or(reply.detail)
    })
    .await
    .map_err(|e| e.to_string())?;

    #[cfg(not(target_os = "android"))]
    {
        let Some(file) = rfd::AsyncFileDialog::new().pick_file().await else {
            return Ok(None);
        };
        let text = String::from_utf8(file.read().await).map_err(|e| e.to_string())?;
        Ok(Some(PickedFile {
            name: file.file_name(),
            text,
        }))
    }
}
