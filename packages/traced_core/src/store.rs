//! Local persistence: one SQLite file for everything the app keeps.

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use tracing::warn;
use uuid::Uuid;

use crate::{
    gps::GPSFix,
    recorder::RecordingSummary,
    speedometer::SpeedometerSnapshot,
    track::{Session, Track, TrackBuildError, TrackBuilder},
};

/// Schema upgrades, one per version (SQLite's `user_version`). Append only: never edit a shipped one.
const MIGRATIONS: &[&str] = &[
    "CREATE TABLE tracks (id TEXT PRIMARY KEY, spec TEXT NOT NULL) STRICT",
    // A track's recordings go with it: their laps mean nothing without its gates.
    "CREATE TABLE recordings (
        id TEXT PRIMARY KEY,
        started_at INTEGER NOT NULL, -- Unix ms
        track_id TEXT REFERENCES tracks (id) ON DELETE CASCADE,
        completed TEXT, -- JSON `Completed`, present with track_id
        speedometer TEXT NOT NULL -- JSON `SpeedometerSnapshot`
    ) STRICT;
    CREATE INDEX recordings_by_start ON recordings (started_at);
    CREATE TABLE fixes (
        recording_id TEXT NOT NULL REFERENCES recordings (id) ON DELETE CASCADE,
        time_ms INTEGER NOT NULL,
        lat REAL NOT NULL,
        lon REAL NOT NULL,
        speed_ms REAL NOT NULL,
        accuracy REAL NOT NULL,
        altitude_m REAL,
        heading_deg REAL,
        vertical_accuracy_m REAL,
        speed_accuracy_ms REAL,
        heading_accuracy_deg REAL
    ) STRICT;
    CREATE INDEX fixes_by_recording ON fixes (recording_id, time_ms);",
];

pub struct Store {
    db: Connection,
}

/// A stored `RecordingSummary`, minus its fixes (see `Store::fixes`).
#[derive(Debug, Clone, PartialEq)]
pub struct SavedRecording {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub session: Option<Session>,
    pub speedometer: SpeedometerSnapshot,
}

#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Track(#[from] TrackBuildError),
    #[error(transparent)]
    Uuid(#[from] uuid::Error),
}

impl Store {
    /// Opens the database at `path`, creating it if needed, and brings its schema up to date.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let mut db = Connection::open(path)?;
        db.pragma_update(None, "foreign_keys", true)?;
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

    /// Deletes the track and, with it, its recordings.
    pub fn delete_track(&self, id: Uuid) -> Result<(), StoreError> {
        self.db
            .execute("DELETE FROM tracks WHERE id = ?1", [id.to_string()])?;
        Ok(())
    }

    /// Saves a finished recording with all its fixes, under a new id.
    pub fn put_recording(&mut self, r: &RecordingSummary) -> Result<Uuid, StoreError> {
        let id = Uuid::new_v4();
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO recordings VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.to_string(),
                r.started_at.timestamp_millis(),
                r.session.as_ref().map(|s| s.track_id.to_string()),
                r.session
                    .as_ref()
                    .map(|s| serde_json::to_string(&s.completed))
                    .transpose()?,
                serde_json::to_string(&r.speedometer)?,
            ],
        )?;
        let mut insert =
            tx.prepare("INSERT INTO fixes VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)")?;
        for f in &r.fixes {
            insert.execute(params![
                id.to_string(),
                f.timestamp.timestamp_millis(),
                f.latitude,
                f.longitude,
                f.speed_ms,
                f.accuracy,
                f.altitude_m,
                f.heading_deg,
                f.vertical_accuracy_m,
                f.speed_accuracy_ms,
                f.heading_accuracy_deg,
            ])?;
        }
        drop(insert);
        tx.commit()?;
        Ok(id)
    }

    /// Every recording, newest first, without its fixes.
    pub fn recordings(&self) -> Result<Vec<SavedRecording>, StoreError> {
        let mut stmt = self.db.prepare(
            "SELECT id, started_at, track_id, completed, speedometer FROM recordings ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (id, started_at, track_id, completed, speedometer) = row?;
            let session = match (track_id, completed) {
                (Some(track_id), Some(completed)) => Some(Session {
                    track_id: Uuid::parse_str(&track_id)?,
                    completed: serde_json::from_str(&completed)?,
                }),
                _ => None,
            };
            Ok(SavedRecording {
                id: Uuid::parse_str(&id)?,
                started_at: DateTime::from_timestamp_millis(started_at).unwrap_or_default(),
                session,
                speedometer: serde_json::from_str(&speedometer)?,
            })
        })
        .collect()
    }

    /// A recording's fixes, in order.
    pub fn fixes(&self, recording: Uuid) -> Result<Vec<GPSFix>, StoreError> {
        let mut stmt = self.db.prepare(
            "SELECT time_ms, lat, lon, speed_ms, accuracy, altitude_m, heading_deg, vertical_accuracy_m,
                speed_accuracy_ms, heading_accuracy_deg
            FROM fixes WHERE recording_id = ?1 ORDER BY time_ms, rowid",
        )?;
        let fixes = stmt.query_map([recording.to_string()], |r| {
            Ok(GPSFix {
                timestamp: DateTime::from_timestamp_millis(r.get(0)?).unwrap_or_default(),
                latitude: r.get(1)?,
                longitude: r.get(2)?,
                speed_ms: r.get(3)?,
                accuracy: r.get(4)?,
                altitude_m: r.get(5)?,
                heading_deg: r.get(6)?,
                vertical_accuracy_m: r.get(7)?,
                speed_accuracy_ms: r.get(8)?,
                heading_accuracy_deg: r.get(9)?,
            })
        })?;
        Ok(fixes.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        speedometer::tests::fix,
        track::{Lap, TrackShape, Waypoint},
    };
    use chrono::TimeDelta;

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

    /// Started at `secs`; with a track, one 30 s run.
    fn summary(track: Option<&Track>, secs: i64) -> RecordingSummary {
        let at = |s| DateTime::UNIX_EPOCH + TimeDelta::seconds(s);
        let session = track.map(|t| {
            let mut session = Session::new(t);
            session.record(Lap::new(vec![at(secs), at(secs + 30)], false));
            session
        });
        let speedometer = SpeedometerSnapshot {
            top_speed_ms: 12.5,
            ..Default::default()
        };
        let fixes = vec![
            fix(secs, 1.0),
            GPSFix {
                altitude_m: Some(80.0),
                ..fix(secs + 1, 2.0)
            },
        ];
        RecordingSummary {
            started_at: at(secs),
            session,
            speedometer,
            fixes,
        }
    }

    #[test]
    fn recordings_round_trip_newest_first() {
        let mut store = Store::open(":memory:").unwrap();
        let spec = sprint("Sprint");
        store.put_track(&spec).unwrap();
        let timed = summary(Some(&spec.build().unwrap()), 100);
        let timed_id = store.put_recording(&timed).unwrap();
        store.put_recording(&summary(None, 200)).unwrap();

        let saved = store.recordings().unwrap();
        assert_eq!(saved.len(), 2);
        assert!(saved[0].session.is_none());
        let expected = SavedRecording {
            id: timed_id,
            started_at: timed.started_at,
            session: timed.session,
            speedometer: timed.speedometer,
        };
        assert_eq!(saved[1], expected);
        assert_eq!(store.fixes(timed_id).unwrap(), timed.fixes);
    }

    #[test]
    fn deleting_a_track_deletes_its_recordings() {
        let mut store = Store::open(":memory:").unwrap();
        let spec = sprint("Sprint");
        store.put_track(&spec).unwrap();
        let timed = store
            .put_recording(&summary(Some(&spec.clone().build().unwrap()), 100))
            .unwrap();
        store.put_recording(&summary(None, 200)).unwrap();

        store.delete_track(spec.uuid()).unwrap();
        assert_eq!(store.recordings().unwrap().len(), 1);
        assert!(store.fixes(timed).unwrap().is_empty());
    }
}
