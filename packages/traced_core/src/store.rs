//! Local persistence: one SQLite file for everything the app keeps.

use std::path::Path;

use rusqlite::{Connection, params};
use tracing::warn;
use uuid::Uuid;

use crate::track::{Track, TrackBuildError, TrackBuilder};

/// Schema upgrades, one per version (SQLite's `user_version`). Append only: never edit a shipped one.
const MIGRATIONS: &[&str] =
    &["CREATE TABLE tracks (id TEXT PRIMARY KEY, spec TEXT NOT NULL) STRICT"];

pub struct Store {
    db: Connection,
}

#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Track(#[from] TrackBuildError),
}

impl Store {
    /// Opens the database at `path`, creating it if needed, and brings its schema up to date.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let mut db = Connection::open(path)?;
        let current: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let tx = db.transaction()?;
        for (sql, version) in MIGRATIONS.iter().zip(1..).skip(current as usize) {
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", version)?;
        }
        tx.commit()?;
        Ok(Self { db })
    }

    /// Inserts or updates the track with `track`'s uuid. Refuses one that doesn't build.
    pub fn put_track(&self, track: &TrackBuilder) -> Result<(), StoreError> {
        track.clone().build()?;
        // Not INSERT OR REPLACE: that deletes the row first, which would cascade to its sessions.
        self.db.execute(
            "INSERT INTO tracks (id, spec) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET spec = excluded.spec",
            params![track.uuid().to_string(), serde_json::to_string(track)?],
        )?;
        Ok(())
    }

    /// Every track, oldest first. One that no longer builds is skipped, not fatal.
    pub fn tracks(&self) -> Result<Vec<Track>, StoreError> {
        let mut stmt = self
            .db
            .prepare("SELECT id, spec FROM tracks ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut tracks = Vec::new();
        for row in rows {
            let (id, spec) = row?;
            let track = serde_json::from_str::<TrackBuilder>(&spec)
                .map_err(StoreError::from)
                .and_then(|b| Ok(b.build()?));
            match track {
                Ok(track) => tracks.push(track),
                Err(e) => warn!("skipping track {id}: {e}"),
            }
        }
        Ok(tracks)
    }

    pub fn delete_track(&self, id: Uuid) -> Result<(), StoreError> {
        self.db
            .execute("DELETE FROM tracks WHERE id = ?1", [id.to_string()])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{TrackShape, Waypoint};

    fn sprint(name: &str) -> TrackBuilder {
        let (start, finish) = (Waypoint::new(6.0, 49.0), Waypoint::new(6.0, 49.001));
        Track::builder(name, TrackShape::Open { start, finish }).waypoints([start, finish])
    }

    #[test]
    fn tracks_survive_reopening_with_their_uuid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("traced.db");
        let track = sprint("Sprint");
        Store::open(&path).unwrap().put_track(&track).unwrap();

        let store = Store::open(&path).unwrap();
        let tracks = store.tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(
            (tracks[0].uuid, tracks[0].name.as_str()),
            (track.uuid(), "Sprint")
        );
        assert!((tracks[0].length() - 111.0).abs() < 1.0);
    }

    #[test]
    fn put_updates_in_place_and_delete_removes() {
        let store = Store::open(":memory:").unwrap();
        let (a, b) = (sprint("A"), sprint("B"));
        store.put_track(&a).unwrap();
        store.put_track(&b).unwrap();
        store
            .put_track(&sprint("A renamed").with_uuid(a.uuid()))
            .unwrap();
        let names = |s: &Store| {
            s.tracks()
                .unwrap()
                .into_iter()
                .map(|t| t.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&store), ["A renamed", "B"]);

        store.delete_track(a.uuid()).unwrap();
        assert_eq!(names(&store), ["B"]);
    }

    #[test]
    fn refuses_and_skips_unbuildable_tracks() {
        let store = Store::open(":memory:").unwrap();
        let lone = Track::builder(
            "Lone",
            TrackShape::Closed {
                gate: Waypoint::new(0.0, 0.0),
            },
        );
        assert!(matches!(store.put_track(&lone), Err(StoreError::Track(_))));

        let spec = serde_json::to_string(&lone).unwrap();
        store
            .db
            .execute(
                "INSERT INTO tracks VALUES ('x', ?1), ('y', 'not json')",
                [spec],
            )
            .unwrap();
        store.put_track(&sprint("Ok")).unwrap();
        assert_eq!(store.tracks().unwrap().len(), 1);
    }
}
