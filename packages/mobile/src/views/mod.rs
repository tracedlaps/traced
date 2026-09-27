mod backup;
mod editor;
mod history;
mod live;
mod map;
mod record;
mod results;
mod shell;
mod track_map;

pub use editor::{EditTrack, Import, NewTrack};
pub use history::{History, SavedResults};
pub use live::Live;
pub use record::{Speedometer, Tracks};
pub use results::Results;
pub use shell::Shell;
use chrono::TimeDelta;
use dioxus::prelude::*;
use traced_core::{gps::GpsStatus, track::Track};

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
        None | Some(GpsStatus::Disconnected) => "GPS off".into(),
        Some(GpsStatus::Connecting) => "GPS connecting…".into(),
        Some(GpsStatus::Searching) => "No GPS fix".into(),
        Some(GpsStatus::Fixed {
            satellites,
            accuracy_m,
            source,
        }) => {
            format!("GPS fix · {source:?} · {satellites} satellites · ±{accuracy_m:.0} m")
        }
    }
}
