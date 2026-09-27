use std::sync::LazyLock;

use traced_core::track::{Track, TrackBuilder, TrackShape, Waypoint};
use uuid::Uuid;

/// A built-in track, seeded into an empty store. Fixed uuids keep `mock_speed_kmh` findable.
pub struct Preset {
    pub track: TrackBuilder,
    /// How fast the mock GPS drives it, until a real provider exists.
    pub mock_speed_kmh: f64,
}

pub static PRESETS: LazyLock<Vec<Preset>> = LazyLock::new(|| {
    // ~170 m x 110 m loop in Lisbon, gate and a sector midway along the first two legs.
    let lisbon = Track::builder(
        "Lisbon test loop",
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
    .sector_gate_at(Waypoint::new(-8.998, 38.7005));

    // Exported waypoints (lat,lon first); sectors start at waypoints 1, 19 and 33.
    let karting = waypoints(include_str!("../tracks/mondercange_acl.csv"));
    let mondercange = Track::builder(
        "Mondercange ACL Karting",
        TrackShape::Closed { gate: karting[0] },
    )
    .sector_gate_at(karting[18])
    .sector_gate_at(karting[32])
    .waypoints(karting);

    // Speedtrike descent, top to bottom in file order; sectors start at waypoints 1, 13 and 28.
    let trike = waypoints(include_str!("../tracks/senhora_da_lage.csv"));
    let shape = TrackShape::Open {
        start: trike[0],
        finish: trike[trike.len() - 1],
    };
    let senhora_da_lage = Track::builder("Senhora da Lage Trike Descent", shape)
        .sector_gate_at(trike[12])
        .sector_gate_at(trike[27])
        .waypoints(trike);

    // Open run, north then a hairpin right; two sectors, split at waypoint 6 (about halfway).
    let run = waypoints(include_str!("../tracks/luxembourg_sprint.csv"));
    let shape = TrackShape::Open {
        start: run[0],
        finish: run[run.len() - 1],
    };
    let sprint = Track::builder("Luxembourg Sprint", shape)
        .sector_gate_at(run[5])
        .waypoints(run);

    [
        (mondercange, 50.0),
        (senhora_da_lage, 90.0),
        (sprint, 60.0),
        (lisbon, 36.0),
    ]
    .into_iter()
    .zip(1..)
    .map(|((track, mock_speed_kmh), id)| Preset {
        track: track.with_uuid(Uuid::from_u128(id)),
        mock_speed_kmh,
    })
    .collect()
});

/// How fast the mock GPS drives `track`: its preset's speed, or 50 km/h.
pub fn mock_speed_kmh(track: &Track) -> f64 {
    PRESETS
        .iter()
        .find(|p| p.track.uuid() == track.uuid)
        .map_or(50.0, |p| p.mock_speed_kmh)
}

/// Waypoints from a CSV export with a header and `lat,lon` as its first columns.
fn waypoints(csv: &str) -> Vec<Waypoint> {
    csv.lines()
        .skip(1)
        .map(|line| {
            let mut cols = line.split(',').map(|c| c.parse().expect("numeric lat,lon"));
            let (lat, lon) = (cols.next().unwrap(), cols.next().unwrap());
            Waypoint::new(lon, lat)
        })
        .collect()
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

        let sprint = &built[2];
        let sectors: Vec<_> = sprint.sectors().map(|s| s.length.round()).collect();
        assert_eq!(sectors.len(), 2);
        assert!((sprint.length() - 610.0).abs() < 5.0, "{sectors:?}");
    }
}
