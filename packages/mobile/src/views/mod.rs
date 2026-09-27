mod backup;
mod capture;
mod editor;
mod history;
mod live;
mod map;
mod record;
mod results;
mod shell;
mod track_map;

pub use capture::RecordTrack;
pub use editor::{EditTrack, Import, NewTrack};
pub use history::{History, SavedResults};
pub use live::Live;
pub use record::{Speedometer, Tracks};
pub use results::Results;
pub use shell::Shell;
use chrono::TimeDelta;
use dioxus::prelude::*;
use dioxus_i18n::t;
use traced_core::{
    gps::{GpsError, GpsStatus},
    track::{import::ImportError, Track, TrackBuildError},
};

/// Why the GPS couldn't start, for the user.
fn refusal(e: GpsError) -> String {
    match e {
        GpsError::PermissionDenied => t!("gps-permission-denied"),
        GpsError::LocationDisabled => t!("gps-location-disabled"),
        GpsError::DeviceNotFound => t!("gps-no-device"),
        GpsError::Unavailable(detail) => t!("gps-unavailable", detail: detail),
    }
}

/// Why a track doesn't build, for the user.
fn track_error(e: &TrackBuildError) -> String {
    match *e {
        TrackBuildError::TooFewWaypoints => t!("too-few-waypoints"),
        TrackBuildError::PlacementOffTrack { distance } => t!("placement-off-track", distance: format!("{distance:.0}")),
        TrackBuildError::PlacementBeyondTrack { overshoot } => t!("placement-beyond-track", distance: format!("{overshoot:.0}")),
        TrackBuildError::FinishBeforeStart => t!("finish-before-start"),
        TrackBuildError::SectorOutsideTrack { .. } => t!("sector-outside"),
    }
}

/// Why a file doesn't import, for the user.
fn import_error(e: &ImportError) -> String {
    match e {
        ImportError::Unrecognised => t!("import-unrecognised"),
        ImportError::BadLine(line) => t!("import-bad-line", line: *line),
        ImportError::Xml(e) => t!("import-bad-xml", detail: e.to_string()),
    }
}

/// `m:ss.t`, the one format for every lap, split and clock.
fn lap_time(secs: f64) -> String {
    let tenths = (secs.max(0.0) * 10.0).floor() as u64;
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

/// Signed seconds against the best: green when ahead (negative), red when behind.
#[component]
fn Delta(delta: TimeDelta) -> Element {
    let secs = delta.as_seconds_f64();
    rsx! {
        span { class: if secs < 0.0 { "delta ahead" } else { "delta behind" }, "{secs:+.2}" }
    }
}

/// Start to finish (m), or a lap: what's timed, without lead-in or lead-out waypoints.
fn timed_m(track: &Track) -> f64 {
    track.sectors().map(|s| s.length).sum()
}

fn kmh(speed_ms: impl Into<f64>) -> String {
    format!("{:.0}", speed_ms.into() * 3.6)
}

fn gps_status(status: Option<GpsStatus>) -> String {
    match status {
        None | Some(GpsStatus::Disconnected) => t!("gps-off"),
        Some(GpsStatus::Connecting) => t!("gps-connecting"),
        Some(GpsStatus::Searching) => t!("gps-no-fix"),
        Some(GpsStatus::Fixed { satellites, accuracy_m, source }) => {
            t!("gps-fix", source: format!("{source:?}"), satellites: satellites, accuracy: format!("{accuracy_m:.0}"))
        }
    }
}
