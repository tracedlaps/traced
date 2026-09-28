use std::sync::LazyLock;

use traced_core::track::{import, Track, TrackBuilder, TrackShape, Waypoint};
use uuid::Uuid;

/// A built-in track, seeded into an empty store. Fixed uuids keep `mock_speed_kmh` findable.
pub struct Preset {
    pub track: TrackBuilder,
    /// The mock GPS's top speed on it; it slows for the corners.
    pub mock_speed_kmh: f64,
}

/// Bump when a preset's name or tags change, so stores seeded earlier pick it up.
pub const PRESETS_VERSION: &str = "2";

pub static PRESETS: LazyLock<Vec<Preset>> = LazyLock::new(|| {
    // Exported waypoints (lat,lon first); sectors start at waypoints 1, 19 and 33.
    let karting = waypoints(include_str!("../tracks/mondercange_acl.csv"));
    let mondercange = Track::builder(
        "Mondercange ACL Karting",
        TrackShape::Closed { gate: karting[0] },
    )
    .tags(["karting"])
    .sector_gate_at(karting[18])
    .sector_gate_at(karting[32])
    .waypoints(karting);

    // Speedtrike descent, top to bottom in file order; sectors start at waypoints 1, 13 and 28.
    let trike = waypoints(include_str!("../tracks/provisende.csv"));
    let shape = TrackShape::Open {
        start: trike[0],
        finish: trike[trike.len() - 1],
    };
    let provisende = Track::builder("Descida de Provisende", shape)
        .tags(["trike", "speedtrike"])
        .sector_gate_at(trike[12])
        .sector_gate_at(trike[27])
        .waypoints(trike);

    // Ids 3 and 4 were the Luxembourg Sprint and Lisbon test loop: don't reuse them.
    [(mondercange, 70.0), (provisende, 100.0)]
        .into_iter()
        .zip(1..)
        .map(|((track, mock_speed_kmh), id)| Preset {
            track: track.with_uuid(Uuid::from_u128(id)),
            mock_speed_kmh,
        })
        .collect()
});

/// The mock GPS's top speed on `track`: its preset's, or 60 km/h.
pub fn mock_speed_kmh(track: &Track) -> f64 {
    PRESETS
        .iter()
        .find(|p| p.track.uuid() == track.uuid)
        .map_or(60.0, |p| p.mock_speed_kmh)
}

/// A bundled CSV export's waypoints.
fn waypoints(csv: &str) -> Vec<Waypoint> {
    import::waypoints(csv).expect("valid bundled track")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_build() {
        let built: Vec<_> = PRESETS
            .iter()
            .map(|p| p.track.clone().build().unwrap())
            .collect();
        let mondercange = &built[0];
        let sectors: Vec<_> = mondercange.sectors().map(|s| s.length.round()).collect();
        assert_eq!(mondercange.waypoints.len(), 41);
        assert_eq!(sectors.len(), 3);
        assert!((mondercange.length() - 856.0).abs() < 5.0, "{sectors:?}");

        let descent = &built[1];
        assert_eq!(descent.sectors().count(), 3);
        assert!((descent.length() - 2553.0).abs() < 5.0);
    }
}
