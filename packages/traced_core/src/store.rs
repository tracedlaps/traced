//! Local persistence: one SQLite file for everything the app keeps.

use std::{path::Path, sync::Arc};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension, backup::Progress, params};
use tracing::warn;
use uuid::Uuid;

use crate::{
    gps::GPSFix,
    imu::{Alignment, GForces, ImuSample},
    recorder::RecordingSummary,
    speedometer::SpeedometerSnapshot,
    timing::{Reference, best_lap},
    track::{Session, Track, TrackBuildError, TrackBuilder},
};

/// Runs the migrations `db` hasn't had yet, all in one transaction.
fn migrate(db: &mut Connection) -> Result<(), StoreError> {
    let current: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let tx = db.transaction()?;
    for (sql, version) in MIGRATIONS.iter().zip(1..).skip(current as usize) {
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
    }
    tx.commit()?;
    Ok(())
}

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
    // Fixes are saved as they come in: a recording without `ended_at` is live, or was cut short.
    "ALTER TABLE recordings ADD COLUMN ended_at INTEGER; -- Unix ms, its last fix's
    ALTER TABLE recordings ADD COLUMN interrupted INTEGER NOT NULL DEFAULT 0; -- recovered after the app died
    UPDATE recordings SET ended_at =
        COALESCE((SELECT max(time_ms) FROM fixes WHERE recording_id = recordings.id), started_at);",
    // Accelerometer samples, saved as they come in like fixes (m/s², device axes).
    "CREATE TABLE imu_samples (
        recording_id TEXT NOT NULL REFERENCES recordings (id) ON DELETE CASCADE,
        time_ms INTEGER NOT NULL,
        ax REAL NOT NULL, ay REAL NOT NULL, az REAL NOT NULL,
        gx REAL NOT NULL, gy REAL NOT NULL, gz REAL NOT NULL
    ) STRICT;
    CREATE INDEX imu_samples_by_recording ON imu_samples (recording_id, time_ms);
    ALTER TABLE recordings ADD COLUMN g TEXT; -- JSON `GForces`, with an accelerometer",
    // User preferences, so they go with a backup.
    "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT",
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
    /// `None` without an accelerometer.
    pub g: Option<GForces>,
    /// Recovered from its saved fixes after the app died mid-recording.
    pub interrupted: bool,
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
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("not a Traced backup")]
    NotABackup,
    #[error("this backup is from a newer version of Traced")]
    BackupTooNew,
}

impl Store {
    /// Opens the database at `path`, creating it if needed, and brings its schema up to date.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let mut db = Connection::open(path)?;
        db.pragma_update(None, "foreign_keys", true)?;
        // Each saved fix is its own commit: WAL makes that cheap and still survives a crash.
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "NORMAL")?;
        migrate(&mut db)?;
        Ok(Self { db })
    }

    /// Writes a compact, consistent copy of everything to `path`, replacing any file there.
    pub fn export(&self, path: impl AsRef<Path>) -> Result<(), StoreError> {
        let path = path.as_ref();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        self.db
            .execute("VACUUM INTO ?1", [path.to_string_lossy()])?;
        Ok(())
    }

    /// Replaces everything with the backup at `path` (from `export`), upgrading an older one.
    /// Leaves the store untouched if `path` isn't a backup this version can read.
    pub fn restore(&mut self, path: impl AsRef<Path>) -> Result<(), StoreError> {
        let backup = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let version: i64 = backup
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(|_| StoreError::NotABackup)?;
        match version {
            0 => return Err(StoreError::NotABackup),
            v if v > MIGRATIONS.len() as i64 => return Err(StoreError::BackupTooNew),
            _ => drop(backup),
        }
        self.db.restore("main", path, None::<fn(Progress)>)?;
        migrate(&mut self.db)
    }

    /// The stored value of setting `key`, if it was ever set.
    pub fn setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .db
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn put_setting(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.db.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    /// Inserts or updates the track with `track`'s uuid. Refuses one that doesn't build.
    /// Moving an existing track's gates re-times its recordings from their fixes.
    pub fn put_track(&self, track: &TrackBuilder) -> Result<(), StoreError> {
        let built = Arc::new(track.clone().build()?);
        let moved = self.track(track.uuid())?.is_some_and(|old| !old.same_geometry(track));
        let tx = self.db.unchecked_transaction()?;
        // Not INSERT OR REPLACE: that deletes the row first, which would cascade to its sessions.
        tx.execute(
            "INSERT INTO tracks (id, spec) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET spec = excluded.spec",
            params![track.uuid().to_string(), serde_json::to_string(track)?],
        )?;
        if moved {
            self.retime(&built)?;
        }
        Ok(tx.commit()?)
    }

    /// The stored spec of track `id`, unbuilt, for editing.
    pub fn track(&self, id: Uuid) -> Result<Option<TrackBuilder>, StoreError> {
        let spec: Option<String> = self
            .db
            .query_row("SELECT spec FROM tracks WHERE id = ?1", [id.to_string()], |r| r.get(0))
            .optional()?;
        Ok(spec.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    /// Replays each finished recording of `track` against its current gates.
    fn retime(&self, track: &Arc<Track>) -> Result<(), StoreError> {
        let recordings = self
            .db
            .prepare("SELECT id, started_at FROM recordings WHERE track_id = ?1 AND ended_at IS NOT NULL")?
            .query_map([track.uuid.to_string()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (id, started_at) in recordings {
            let uuid = Uuid::parse_str(&id)?;
            let summary = RecordingSummary::replay(
                DateTime::from_timestamp_millis(started_at).unwrap_or_default(),
                self.fixes(uuid)?,
                self.samples(uuid)?,
                Some(track.clone()),
            );
            let completed = summary.session.map(|s| serde_json::to_string(&s.completed));
            // Lap peaks follow the laps.
            let g = summary.g.map(|g| serde_json::to_string(&g));
            self.db.execute(
                "UPDATE recordings SET completed = ?2, g = ?3 WHERE id = ?1",
                params![id, completed.transpose()?, g.transpose()?],
            )?;
        }
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

    /// Starts saving a recording, under a new id; its fixes follow through `append_fix`.
    pub fn begin_recording(
        &self,
        started_at: DateTime<Utc>,
        track: Option<Uuid>,
    ) -> Result<Uuid, StoreError> {
        let id = Uuid::new_v4();
        self.db.execute(
            "INSERT INTO recordings (id, started_at, track_id, speedometer) VALUES (?1, ?2, ?3, ?4)",
            params![
                id.to_string(),
                started_at.timestamp_millis(),
                track.map(|t| t.to_string()),
                serde_json::to_string(&SpeedometerSnapshot::default())?,
            ],
        )?;
        Ok(id)
    }

    pub fn append_fix(&self, recording: Uuid, f: &GPSFix) -> Result<(), StoreError> {
        self.db.execute(
            "INSERT INTO fixes VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                recording.to_string(),
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
            ],
        )?;
        Ok(())
    }

    pub fn append_sample(&self, recording: Uuid, s: &ImuSample) -> Result<(), StoreError> {
        let ([ax, ay, az], [gx, gy, gz]) = (s.accel, s.gravity);
        self.db.execute(
            "INSERT INTO imu_samples VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![recording.to_string(), s.timestamp.timestamp_millis(), ax, ay, az, gx, gy, gz],
        )?;
        Ok(())
    }

    /// Stores how recording `id` went, or drops it if it never got a fix.
    pub fn finish_recording(
        &self,
        id: Uuid,
        r: &RecordingSummary,
        interrupted: bool,
    ) -> Result<(), StoreError> {
        let Some(last) = r.fixes.last() else {
            self.db
                .execute("DELETE FROM recordings WHERE id = ?1", [id.to_string()])?;
            return Ok(());
        };
        self.db.execute(
            "UPDATE recordings SET completed = ?2, speedometer = ?3, ended_at = ?4, interrupted = ?5, g = ?6
            WHERE id = ?1",
            params![
                id.to_string(),
                r.session
                    .as_ref()
                    .map(|s| serde_json::to_string(&s.completed))
                    .transpose()?,
                serde_json::to_string(&r.speedometer)?,
                last.timestamp.timestamp_millis(),
                interrupted,
                r.g.as_ref().map(serde_json::to_string).transpose()?,
            ],
        )?;
        Ok(())
    }

    /// Finishes every recording the app died during, from the fixes it saved. Call before recording.
    pub fn recover(&self) -> Result<(), StoreError> {
        let unfinished = self
            .db
            .prepare("SELECT id, started_at, track_id FROM recordings WHERE ended_at IS NULL")?
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if unfinished.is_empty() {
            return Ok(());
        }
        let tracks: Vec<_> = self.tracks()?.into_iter().map(Arc::new).collect();
        for (id, started_at, track_id) in unfinished {
            let id = Uuid::parse_str(&id)?;
            let track = track_id.and_then(|t| tracks.iter().find(|x| x.uuid.to_string() == t));
            let summary = RecordingSummary::replay(
                DateTime::from_timestamp_millis(started_at).unwrap_or_default(),
                self.fixes(id)?,
                self.samples(id)?,
                track.cloned(),
            );
            self.finish_recording(id, &summary, true)?;
        }
        Ok(())
    }

    /// Every recording, newest first, without its fixes.
    pub fn recordings(&self) -> Result<Vec<SavedRecording>, StoreError> {
        let mut stmt = self.db.prepare(
            "SELECT id, started_at, track_id, completed, speedometer, interrupted, g FROM recordings
            WHERE ended_at IS NOT NULL ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, bool>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })?;
        rows.map(|row| {
            let (id, started_at, track_id, completed, speedometer, interrupted, g) = row?;
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
                g: g.map(|g| serde_json::from_str(&g)).transpose()?,
                interrupted,
            })
        })
        .collect()
    }

    /// The best lap on `track` across its recordings, to race against; `None` before any.
    pub fn reference(&self, track: &Track) -> Result<Option<Reference>, StoreError> {
        let recordings = self.recordings()?;
        let Some((id, lap)) = best_lap(&recordings, track.uuid) else { return Ok(None) };
        Ok(Some(Reference::new(track, lap.clone(), &self.fixes(id)?)))
    }

    /// The phone's alignment in its mount as the latest recording that found one left it.
    pub fn last_alignment(&self) -> Result<Option<Alignment>, StoreError> {
        let alignment: Option<String> = self
            .db
            .query_row(
                "SELECT json_extract(g, '$.alignment') FROM recordings
                WHERE json_extract(g, '$.alignment') IS NOT NULL ORDER BY started_at DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(alignment.map(|a| serde_json::from_str(&a)).transpose()?)
    }

    /// A recording's accelerometer samples, in order.
    pub fn samples(&self, recording: Uuid) -> Result<Vec<ImuSample>, StoreError> {
        let mut stmt = self.db.prepare(
            "SELECT time_ms, ax, ay, az, gx, gy, gz FROM imu_samples WHERE recording_id = ?1 ORDER BY time_ms, rowid",
        )?;
        let samples = stmt.query_map([recording.to_string()], |r| {
            Ok(ImuSample {
                timestamp: DateTime::from_timestamp_millis(r.get(0)?).unwrap_or_default(),
                accel: [r.get(1)?, r.get(2)?, r.get(3)?],
                gravity: [r.get(4)?, r.get(5)?, r.get(6)?],
            })
        })?;
        Ok(samples.collect::<Result<_, _>>()?)
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
        imu::GPeaks,
        speedometer::tests::fix,
        track::{Completed, Lap, TrackShape, Waypoint},
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
        let track = sprint("Sprint").tags(["trike"]);
        Store::open(&path).unwrap().put_track(&track).unwrap();

        let store = Store::open(&path).unwrap();
        let tracks = store.tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(
            (tracks[0].uuid, tracks[0].name.as_str()),
            (track.uuid(), "Sprint")
        );
        assert_eq!(tracks[0].tags, ["trike"]);
        assert!((tracks[0].length() - 111.0).abs() < 1.0);
    }

    #[test]
    fn specs_saved_before_tags_load_untagged() {
        let store = Store::open(":memory:").unwrap();
        let spec = serde_json::to_string(&sprint("Old")).unwrap(); // untagged: no "tags" key
        assert!(!spec.contains("tags"));
        store
            .db
            .execute("INSERT INTO tracks VALUES ('00000000-0000-0000-0000-000000000001', ?1)", [spec])
            .unwrap();
        assert!(store.tracks().unwrap()[0].tags.is_empty());
    }

    #[test]
    fn settings_are_unset_until_put_then_overwritten() {
        let store = Store::open(":memory:").unwrap();
        assert_eq!(store.setting("units").unwrap(), None);
        store.put_setting("units", "imperial").unwrap();
        store.put_setting("units", "metric").unwrap();
        assert_eq!(store.setting("units").unwrap().as_deref(), Some("metric"));
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
    fn moving_a_gate_retimes_recordings_and_renaming_does_not() {
        let store = Store::open(":memory:").unwrap();
        let w = [49.0, 49.0005, 49.001].map(|lat| Waypoint::new(6.0, lat));
        let open = |name, start| {
            Track::builder(name, TrackShape::Open { start, finish: w[2] }).waypoints(w)
        };
        let spec = open("Sprint", w[0]);
        store.put_track(&spec).unwrap();
        // North at ~11 m/s, from just before the start to just past the finish.
        let fixes: Vec<_> = (0..13)
            .map(|s| GPSFix {
                latitude: 48.99995 + 0.0001 * s as f64,
                longitude: 6.0,
                ..fix(s, 11.0)
            })
            .collect();
        let at = fixes[0].timestamp;
        let built = Arc::new(spec.clone().build().unwrap());
        save(&store, &RecordingSummary::replay(at, fixes.clone(), Vec::new(), Some(built)));
        let session = || store.recordings().unwrap()[0].session.clone().unwrap();
        let before = session();
        assert!(matches!(before.completed, Completed::Run(Some(_))), "{before:?}");

        store.put_track(&open("Renamed", w[0]).with_uuid(spec.uuid())).unwrap();
        assert_eq!(session(), before);

        let moved = open("Renamed", w[1]).with_uuid(spec.uuid());
        store.put_track(&moved).unwrap();
        let replayed = RecordingSummary::replay(at, fixes, Vec::new(), Some(Arc::new(moved.build().unwrap())));
        assert_ne!(session(), before);
        assert_eq!(Some(session()), replayed.session);
        assert_eq!(store.track(spec.uuid()).unwrap().unwrap().name(), "Renamed");
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
        let sample = |s, x| ImuSample { timestamp: at(s), accel: [x, 0.5, 0.0], gravity: [0.0, 0.0, 9.8] };
        RecordingSummary {
            started_at: at(secs),
            g: Some(GForces {
                peaks: GPeaks { braking: 0.75, ..Default::default() },
                laps: vec![GPeaks::default()],
                alignment: None,
            }),
            samples: vec![sample(secs, 1.0), sample(secs + 1, -2.0)],
            session,
            speedometer,
            trace: None,
            fixes,
        }
    }

    /// Saves `r` the way a live recording does.
    fn save(store: &Store, r: &RecordingSummary) -> Uuid {
        let id = store
            .begin_recording(r.started_at, r.session.as_ref().map(|s| s.track_id))
            .unwrap();
        r.fixes.iter().for_each(|f| store.append_fix(id, f).unwrap());
        r.samples.iter().for_each(|s| store.append_sample(id, s).unwrap());
        store.finish_recording(id, r, false).unwrap();
        id
    }

    #[test]
    fn reference_is_the_best_lap_with_its_fixes() {
        use crate::track::correlator::tests::{around, fix, square_spec};
        let store = Store::open(":memory:").unwrap();
        let spec = square_spec(&[]);
        store.put_track(&spec).unwrap();
        let track = Arc::new(spec.build().unwrap());
        assert_eq!(store.reference(&track).unwrap(), None);
        // Two laps each, at 8 m/s then 10 m/s: 50 s and 40 s laps.
        for speed in [8.0, 10.0] {
            let fixes = (0..90).map(|i| fix(around(25.0 + 10.0 * i as f64), (25.0 + 10.0 * i as f64) / speed)).collect();
            save(&store, &RecordingSummary::replay(Utc::now(), fixes, Vec::new(), Some(track.clone())));
        }
        let r = store.reference(&track).unwrap().unwrap();
        assert_eq!(r.lap.duration(), TimeDelta::seconds(40));
        assert!((r.time_at(100.0) - TimeDelta::seconds(10)).abs() < TimeDelta::milliseconds(1));
    }

    #[test]
    fn recordings_round_trip_newest_first() {
        let store = Store::open(":memory:").unwrap();
        let spec = sprint("Sprint");
        store.put_track(&spec).unwrap();
        let timed = summary(Some(&spec.build().unwrap()), 100);
        let timed_id = save(&store, &timed);
        save(&store, &summary(None, 200));

        let saved = store.recordings().unwrap();
        assert_eq!(saved.len(), 2);
        assert!(saved[0].session.is_none());
        let expected = SavedRecording {
            id: timed_id,
            started_at: timed.started_at,
            session: timed.session,
            speedometer: timed.speedometer,
            g: timed.g,
            interrupted: false,
        };
        assert_eq!(saved[1], expected);
        assert_eq!(store.fixes(timed_id).unwrap(), timed.fixes);
        assert_eq!(store.samples(timed_id).unwrap(), timed.samples);
    }

    #[test]
    fn last_alignment_is_the_newest_found() {
        let store = Store::open(":memory:").unwrap();
        assert_eq!(store.last_alignment().unwrap(), None);
        let aligned = |forward| Some(Alignment { up: [0.0, 0.0, 1.0], forward });
        for (secs, alignment) in [(100, aligned([1.0, 0.0, 0.0])), (200, aligned([0.0, 1.0, 0.0])), (300, None)] {
            let mut r = summary(None, secs);
            r.g.as_mut().unwrap().alignment = alignment;
            save(&store, &r);
        }
        assert_eq!(store.last_alignment().unwrap(), aligned([0.0, 1.0, 0.0]));
    }

    #[test]
    fn recover_finishes_what_the_app_died_during() {
        let store = Store::open(":memory:").unwrap();
        let spec = sprint("Sprint");
        store.put_track(&spec).unwrap();
        let cut = summary(None, 100);
        let id = store.begin_recording(cut.started_at, Some(spec.uuid())).unwrap();
        cut.fixes.iter().for_each(|f| store.append_fix(id, f).unwrap());
        cut.samples.iter().for_each(|s| store.append_sample(id, s).unwrap());
        store.begin_recording(cut.started_at, None).unwrap(); // died before any fix
        assert!(store.recordings().unwrap().is_empty());

        store.recover().unwrap();
        let replayed =
            RecordingSummary::replay(cut.started_at, cut.fixes.clone(), cut.samples.clone(), Some(Arc::new(spec.build().unwrap())));
        let expected = SavedRecording {
            id,
            started_at: cut.started_at,
            session: replayed.session,
            speedometer: replayed.speedometer,
            g: replayed.g,
            interrupted: true,
        };
        assert_eq!(store.recordings().unwrap(), [expected]);
        assert_eq!(store.fixes(id).unwrap(), cut.fixes);
    }

    #[test]
    fn upgrading_keeps_recordings_saved_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v2.db");
        let old = Connection::open(&path).unwrap();
        MIGRATIONS[..2].iter().for_each(|m| old.execute_batch(m).unwrap());
        old.pragma_update(None, "user_version", 2).unwrap();
        old.execute_batch(
            "INSERT INTO recordings VALUES ('00000000-0000-0000-0000-000000000001', 0, NULL, NULL, '{\"current_speed_ms\":0,\"top_speed_ms\":0,\"distance_m\":0,\"average_speed_ms\":0,\"elapsed\":{\"secs\":0,\"nanos\":0}}')",
        )
        .unwrap();
        drop(old);

        let store = Store::open(&path).unwrap();
        store.recover().unwrap(); // leaves it alone: it isn't unfinished
        let saved = store.recordings().unwrap();
        assert_eq!(saved.len(), 1);
        assert!(!saved[0].interrupted);
    }

    #[test]
    fn deleting_a_track_deletes_its_recordings() {
        let store = Store::open(":memory:").unwrap();
        let spec = sprint("Sprint");
        store.put_track(&spec).unwrap();
        let timed = save(&store, &summary(Some(&spec.clone().build().unwrap()), 100));
        save(&store, &summary(None, 200));

        store.delete_track(spec.uuid()).unwrap();
        assert_eq!(store.recordings().unwrap().len(), 1);
        assert!(store.fixes(timed).unwrap().is_empty());
    }

    #[test]
    fn restore_replaces_everything_with_an_export() {
        let dir = tempfile::tempdir().unwrap();
        let backup = dir.path().join("backup.db");
        let source = Store::open(":memory:").unwrap();
        let spec = sprint("Sprint");
        source.put_track(&spec).unwrap();
        let id = save(&source, &summary(Some(&spec.clone().build().unwrap()), 100));
        source.export(&backup).unwrap();
        source.export(&backup).unwrap(); // over an existing file

        let mut store = Store::open(":memory:").unwrap();
        store.put_track(&sprint("Replaced")).unwrap();
        store.restore(&backup).unwrap();
        let names: Vec<_> = store
            .tracks()
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, ["Sprint"]);
        assert_eq!(store.recordings().unwrap(), source.recordings().unwrap());
        assert_eq!(store.fixes(id).unwrap().len(), 2);
    }

    #[test]
    fn restore_upgrades_an_older_backup() {
        let dir = tempfile::tempdir().unwrap();
        let backup = dir.path().join("v1.db");
        let old = Connection::open(&backup).unwrap();
        old.execute_batch(MIGRATIONS[0]).unwrap();
        old.pragma_update(None, "user_version", 1).unwrap();
        let spec = serde_json::to_string(&sprint("Old")).unwrap();
        old.execute("INSERT INTO tracks VALUES ('a', ?1)", [spec])
            .unwrap();
        drop(old);

        let mut store = Store::open(":memory:").unwrap();
        store.restore(&backup).unwrap();
        assert_eq!(store.tracks().unwrap()[0].name, "Old");
        assert!(store.recordings().unwrap().is_empty()); // v2's tables exist
    }

    #[test]
    fn restore_refuses_other_files_and_keeps_the_data() {
        let dir = tempfile::tempdir().unwrap();
        let (text, empty, newer) = (
            dir.path().join("t.csv"),
            dir.path().join("e.db"),
            dir.path().join("n.db"),
        );
        std::fs::write(&text, "lat,lon\n1,2\n").unwrap();
        Connection::open(&empty)
            .unwrap()
            .execute_batch("CREATE TABLE x (y)")
            .unwrap();
        Connection::open(&newer)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();

        let mut store = Store::open(":memory:").unwrap();
        store.put_track(&sprint("Kept")).unwrap();
        assert!(matches!(store.restore(&text), Err(StoreError::NotABackup)));
        assert!(matches!(store.restore(&empty), Err(StoreError::NotABackup)));
        assert!(matches!(
            store.restore(&newer),
            Err(StoreError::BackupTooNew)
        ));
        assert_eq!(store.tracks().unwrap()[0].name, "Kept");
    }
}
