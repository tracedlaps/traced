use traced_core::gps::{
    GpsProvider,
    mock::{MockGPSConfig, MockGPSProvider},
};
use traced_core::recorder::Recorder;
use traced_core::track::{LiveStatus, Track, TrackShape, Waypoint};
use std::sync::Arc;
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
    // ~170 m x 110 m loop in Lisbon, gate and a sector midway along the first two legs.
    let track = Track::builder(
        "Test",
        TrackShape::Closed {
            gate: Waypoint::new(-8.999, 38.7),
        },
    )
    .waypoints([
        Waypoint::new(-9.0, 38.7),
        Waypoint::new(-8.998, 38.7),
        Waypoint::new(-8.998, 38.701),
        Waypoint::new(-9.0, 38.701),
    ])
    .sector_gate_at(Waypoint::new(-8.998, 38.7005))
    .build()
    .expect("track");

    tracing::info!(
        "Track created with ID: {} ({:.1} m)",
        track.uuid,
        track.length()
    );
    for (i, sector) in track.sectors().enumerate() {
        tracing::info!("Sector {}: {:.1} m", i + 1, sector.length);
    }

    // Laps of the track at up to 10 m/s, with ±2 m of GPS jitter.
    let config = MockGPSConfig { jitter_m: 2.0, ..Default::default() };
    let mut gps_provider = MockGPSProvider::drive(&track, 10.0, config);
    gps_provider.init().unwrap();

    // Record until the stream ends or Ctrl-C.
    let mut rec = Recorder::start(gps_provider.stream_fixes(), Some(Arc::new(track)), None, false, chrono::Utc::now(), None);
    if let Some(mut live) = rec.session.clone() {
        tokio::spawn(async move {
            while live.changed().await.is_ok() {
                let s = *live.borrow();
                match s.status {
                    LiveStatus::InProgress { sector, .. } => tracing::info!(
                        "Lap {} sector {} (laps done: {}, last: {:?})",
                        s.laps + 1, sector + 1, s.laps, s.last_lap.map(|t| t.as_seconds_f64())
                    ),
                    status => tracing::info!("Session {status:?}"),
                }
            }
        });
    }
    tokio::select! {
        _ = async {
            while rec.speedometer.changed().await.is_ok() {
                let s = *rec.speedometer.borrow();
                tracing::info!(
                    "{:.1} m/s (top {:.1}, avg {:.1}) {:.0} m in {:.0?}",
                    s.current_speed_ms, s.top_speed_ms, s.average_speed_ms, s.distance_m, s.elapsed
                );
            }
        } => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    tracing::info!("Recording finished: {:?}", rec.stop().await);

    gps_provider.deinit();
}
