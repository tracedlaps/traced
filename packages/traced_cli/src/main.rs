use futures::StreamExt;
use traced_core::gps::{GpsProvider, mock::MockGPSProvider};
use traced_core::track::{Track, TrackShape, Waypoint};
use tracing_subscriber::EnvFilter;

pub fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let file = std::env::var("TRACED_LOG_FILE")
        .ok()
        .and_then(|path| std::fs::File::create(path).ok())
        .map(std::sync::Arc::new);

    let installed = match file {
        Some(file) => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_ansi(false)
            .with_writer(file)
            .try_init()
            .is_ok(),
        None => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .try_init()
            .is_ok(),
    };

    if !installed {
        tracing::debug!("logging was already initialised");
    }
}

#[tokio::main]
async fn main() {
    init_logging();
    let mut gps_provider = MockGPSProvider::default();

    gps_provider.init().unwrap();

    // ~170 m x 110 m loop in Lisbon, gate and a sector midway along the first two legs.
    let track = Track::builder("Test", TrackShape::Closed { gate: Waypoint::new(-8.999, 38.7) })
        .waypoints([
            Waypoint::new(-9.0, 38.7),
            Waypoint::new(-8.998, 38.7),
            Waypoint::new(-8.998, 38.701),
            Waypoint::new(-9.0, 38.701),
        ])
        .sector_gate_at(Waypoint::new(-8.998, 38.7005))
        .build()
        .expect("track");

    tracing::info!("Track created with ID: {} ({:.1} m)", track.uuid, track.length());
    for (i, sector) in track.sectors().enumerate() {
        tracing::info!("Sector {}: {:.1} m", i + 1, sector.length);
    }

    let mut fixes = gps_provider.stream_fixes();
    while let Some(fix) = fixes.next().await {
        match fix {
            Ok(fix) => tracing::info!("GPS Fix: {fix}"),
            Err(e) => tracing::error!("GPS error: {e}"),
        }
    }

    gps_provider.deinit();
}
