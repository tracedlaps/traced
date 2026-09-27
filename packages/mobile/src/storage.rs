//! Where the app keeps its data, and the tracks stored there.

use std::{error::Error, fs, path::PathBuf, sync::Arc};

use dioxus::logger::tracing::warn;
use traced_core::{store::Store, track::Track};

use crate::tracks::PRESETS;

#[cfg(target_os = "android")]
mod plugin {
    #[manganis::ffi("android/storage")]
    extern "Kotlin" {
        pub type StoragePlugin;
        pub fn data_dir(this: &StoragePlugin) -> String;
    }
}

/// The app's private files dir on Android, the user's data dir (e.g. `~/.local/share/traced`) elsewhere.
fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    let dir = {
        use plugin::{data_dir, StoragePlugin};
        let plugin = StoragePlugin::from_global_ref(crate::android::load_plugin(
            "dev.traced.storage.StoragePlugin",
        )?);
        data_dir(&plugin).ok().map(PathBuf::from)
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

/// The stored tracks, or the presets alone if the store can't be opened.
pub fn load_tracks() -> Vec<Arc<Track>> {
    let tracks = open()
        .and_then(|store| Ok(store.tracks()?))
        .unwrap_or_else(|e| {
            warn!("track store unavailable, using the presets: {e}");
            PRESETS
                .iter()
                .filter_map(|p| p.track.clone().build().ok())
                .collect()
        });
    tracks.into_iter().map(Arc::new).collect()
}
