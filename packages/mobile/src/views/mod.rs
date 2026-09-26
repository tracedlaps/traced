mod live;
mod results;
mod setup;
mod track_map;

pub use live::Live;
pub use results::Results;
pub use setup::Setup;

/// `m:ss.t`, the one format for every lap, split and clock.
fn lap_time(secs: f64) -> String {
    let tenths = (secs.max(0.0) * 10.0).floor() as u64;
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

fn kmh(speed_ms: impl Into<f64>) -> String {
    format!("{:.0}", speed_ms.into() * 3.6)
}
