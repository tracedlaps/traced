pub(crate) mod correlator;
pub mod import;
mod session;
pub mod trace;
pub use correlator::{Correlator, LiveSessionSnapshot, LiveStatus};
pub use session::{Completed, Lap, Run, Session};

use geo::{Coord, Densify, Euclidean, InterpolateLine, Length, Line, Point, coord};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type Waypoint = geo::Point<f64>;
pub type Polyline = geo::LineString<f64>;
static EARTH_RADIUS: f64 = 6_371_000.0;

#[derive(Debug)]
pub struct Track {
    pub uuid: Uuid,
    pub name: String,
    /// Free-form labels, e.g. the kind of vehicle ("karting", "speedtrike").
    pub tags: Vec<String>,
    pub boundary: Boundary,
    pub waypoints: Vec<Waypoint>,
    polyline: Polyline,
    pub projection: Projection,

    pub sector_gates: Vec<Gate>,
}

impl Track {
    /// No public constructor: a `Gate`'s Cartesian fields can't be derived
    /// before a `Polyline` exists, so `TrackBuilder::build` is the only way
    /// to get a `Track`.
    pub fn builder(name: impl Into<String>, shape: TrackShape) -> TrackBuilder {
        TrackBuilder {
            uuid: Uuid::new_v4(),
            name: name.into(),
            tags: Vec::new(),
            shape,
            waypoints: Vec::new(),
            sector_placements: Vec::new(),
        }
    }

    /// Total polyline length (m); a full lap on a Closed track.
    pub fn length(&self) -> f64 {
        Euclidean.length(&self.polyline)
    }

    /// Start to finish gate (m); a full lap on a Closed track.
    pub fn lap_length(&self) -> f64 {
        self.sectors().map(|s| s.length).sum()
    }

    /// The point `distance` metres along the polyline (clamped to its ends), as lat/lon.
    pub fn point_at(&self, distance: f64) -> Waypoint {
        let p = Euclidean.point_at_distance_from_start(&self.polyline, distance);
        self.projection.unproject(p.expect("a built polyline has ≥ 2 points"))
    }

    /// Distance (m) along the polyline of its point nearest `p` (projected). Searches within
    /// `LOCATE_WINDOW` of `hint`, the last located distance, wrapping on a Closed loop; if
    /// nothing there is within `MAX_PLACEMENT_OFFSET` of `p` (e.g. after a dropout), everywhere.
    pub(crate) fn locate(&self, p: Coord, hint: Option<f64>) -> f64 {
        let length = self.length();
        let near = |d: f64, h: f64| match (&self.boundary, (d - h).abs()) {
            (Boundary::Closed { .. }, gap) => gap.min(length - gap) <= LOCATE_WINDOW,
            (Boundary::Open { .. }, gap) => gap <= LOCATE_WINDOW,
        };
        let search = |hint: Option<f64>| {
            nearest(snaps(&self.polyline, p).filter(|(_, g)| hint.is_none_or(|h| near(g.distance_along_track, h))))
        };
        let (_, gate) = search(hint)
            .filter(|(offset, _)| *offset <= MAX_PLACEMENT_OFFSET)
            .or_else(|| search(None))
            .expect("a built polyline has ≥ 2 points");
        gate.distance_along_track
    }

    /// Start gate, sector gates in travel order, then finish gate.
    /// Closed yields its single gate at both ends: a lap starts and ends on it.
    pub fn gates_ordered(&self) -> impl Iterator<Item = &Gate> {
        let (first, last) = match &self.boundary {
            Boundary::Open { start, finish } => (start, finish),
            Boundary::Closed { gate } => (gate, gate),
        };
        std::iter::once(first).chain(&self.sector_gates).chain(std::iter::once(last))
    }

    /// Consecutive gate pairs from `gates_ordered`, derived on demand.
    pub fn sectors(&self) -> impl Iterator<Item = Sector<'_>> {
        let total = self.length();
        self.gates_ordered().zip(self.gates_ordered().skip(1)).map(move |(from, to)| {
            // rem_euclid handles a Closed loop's wrap past the polyline seam (only there: an
            // Open track's finish on its last point is `total` along, not 0);
            // gate -> same gate (no sectors on a loop) is a full lap, not 0.
            let d = to.distance_along_track - from.distance_along_track;
            let length = match (std::ptr::eq(from, to), &self.boundary) {
                (true, _) => total,
                (false, Boundary::Open { .. }) => d,
                (false, Boundary::Closed { .. }) => d.rem_euclid(total),
            };
            Sector { from, to, length }
        })
    }
}

/// The stretch between two consecutive gates. Borrowed view, never stored.
#[derive(Debug)]
pub struct Sector<'a> {
    pub from: &'a Gate,
    pub to: &'a Gate,
    pub length: f64,
}

/// Where the gates are placed, per track kind.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TrackShape {
    Open { start: Waypoint, finish: Waypoint },
    Closed { gate: Waypoint },
}

/// The built boundary gate(s): `TrackShape`'s placements snapped onto the polyline.
/// Closed has a single gate, so start == finish holds by construction.
#[derive(Debug)]
pub enum Boundary {
    Open { start: Gate, finish: Gate },
    Closed { gate: Gate },
}

/// A track's authored inputs; also its persisted form, since everything a `Track` adds is derived.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackBuilder {
    uuid: Uuid,
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    shape: TrackShape,
    waypoints: Vec<Waypoint>,
    sector_placements: Vec<Waypoint>,
}

impl TrackBuilder {
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tag_list(&self) -> &[String] {
        &self.tags
    }

    pub fn shape(&self) -> TrackShape {
        self.shape
    }

    pub fn waypoint_list(&self) -> &[Waypoint] {
        &self.waypoints
    }

    pub fn sector_placements(&self) -> &[Waypoint] {
        &self.sector_placements
    }

    /// Whether both place the same gates on the same waypoints, whatever their names.
    pub fn same_geometry(&self, other: &Self) -> bool {
        (self.shape, &self.waypoints, &self.sector_placements)
            == (other.shape, &other.waypoints, &other.sector_placements)
    }

    /// Keeps a track's identity across rebuilds; `Track::builder` picks a random one.
    pub fn with_uuid(mut self, uuid: Uuid) -> Self {
        self.uuid = uuid;
        self
    }

    pub fn tags<S: Into<String>>(mut self, tags: impl IntoIterator<Item = S>) -> Self {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    pub fn waypoints(mut self, waypoints: impl IntoIterator<Item = Waypoint>) -> Self {
        self.waypoints = waypoints.into_iter().collect();
        self
    }

    pub fn sector_gate_at(mut self, waypoint: Waypoint) -> Self {
        self.sector_placements.push(waypoint);
        self
    }

    /// Builds the `Polyline` from `waypoints` first, then projects each
    /// placement onto it to derive real `Gate`s, then assembles the `Boundary`.
    pub fn build(self) -> Result<Track, TrackBuildError> {
        if self.waypoints.len() < 2 {
            return Err(TrackBuildError::TooFewWaypoints);
        }
        let projection = Projection::new(self.waypoints[0]);

        let mut polyline: Polyline = self
            .waypoints
            .clone()
            .into_iter()
            .map(|w| projection.project(w))
            .collect();

        // NOTE: Close the loop in case we have a Closed track
        if let TrackShape::Closed { gate: _ } = self.shape {
            polyline.close();
        }
        let polyline = Euclidean.densify(&polyline, DENSIFY_SPACING);
        let length = Euclidean.length(&polyline);

        let place = |w| place_gate(&polyline, projection.project(w));

        let boundary = match self.shape {
            TrackShape::Open { start, finish } => Boundary::Open {
                start: place(start)?,
                finish: place(finish)?,
            },
            TrackShape::Closed { gate } => Boundary::Closed { gate: place(gate)? },
        };
        let (origin, span) = match &boundary {
            Boundary::Open { start, finish }
                if finish.distance_along_track <= start.distance_along_track =>
            {
                return Err(TrackBuildError::FinishBeforeStart);
            }
            Boundary::Open { start, finish } => (
                start.distance_along_track,
                finish.distance_along_track - start.distance_along_track,
            ),
            Boundary::Closed { gate } => (gate.distance_along_track, length),
        };

        // Distance past the start gate in travel direction, wrapping past a loop's seam.
        let from_start = |g: &Gate| (g.distance_along_track - origin).rem_euclid(length);
        let mut sector_gates = self
            .sector_placements
            .into_iter()
            .map(place)
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(g) = sector_gates.iter().find(|g| !(0.0 < from_start(g) && from_start(g) < span)) {
            return Err(TrackBuildError::SectorOutsideTrack {
                distance_along_track: g.distance_along_track,
            });
        }
        sector_gates.sort_by(|a, b| from_start(a).total_cmp(&from_start(b)));

        Ok(Track {
            uuid: self.uuid,
            name: self.name,
            tags: self.tags,
            boundary,
            waypoints: self.waypoints,
            polyline,
            projection,
            sector_gates,
        })
    }
}

/// Max spacing (m) between polyline points after densifying.
const DENSIFY_SPACING: f64 = 2.0;
/// Max distance (m) a placement may sit from the polyline and still snap onto it.
const MAX_PLACEMENT_OFFSET: f64 = 30.0;
const GATE_HALF_WIDTH: f64 = 15.0;
/// How far (m) along the track from its last position `Track::locate` looks first.
const LOCATE_WINDOW: f64 = 50.0;
/// Slack (m) past an open end before a placement counts as beyond it; absorbs float noise.
const END_TOLERANCE: f64 = 1e-6;

/// Snaps `at` (already projected) onto the nearest polyline segment and builds a `Gate` there.
fn place_gate(polyline: &Polyline, at: Point) -> Result<Gate, TrackBuildError> {
    let p = at.0;
    let travelled: f64 = polyline.lines().map(|l| l.delta().x.hypot(l.delta().y)).sum();
    // every segment was zero-length
    let (offset, gate) = nearest(snaps(polyline, p)).ok_or(TrackBuildError::TooFewWaypoints)?;

    // Snapped onto an open end while lying past it: the waypoints don't cover this placement.
    let e = p - gate.center.0;
    let along = e.x * gate.direction.x + e.y * gate.direction.y;
    let past_end = (gate.distance_along_track == 0.0 && along < -END_TOLERANCE)
        || (gate.distance_along_track == travelled && along > END_TOLERANCE);
    if past_end && !polyline.is_closed() {
        return Err(TrackBuildError::PlacementBeyondTrack { overshoot: along.abs() });
    }
    if offset > MAX_PLACEMENT_OFFSET {
        return Err(TrackBuildError::PlacementOffTrack { distance: offset });
    }
    Ok(gate)
}

/// `p` snapped onto each segment, as its offset from `p` and a `Gate` there, accumulating
/// segment lengths for `distance_along_track`.
fn snaps(polyline: &Polyline, p: Coord) -> impl Iterator<Item = (f64, Gate)> + '_ {
    polyline
        .lines()
        .scan(0.0, move |travelled, seg| {
            let d = seg.delta();
            let len = d.x.hypot(d.y);
            let start = std::mem::replace(travelled, *travelled + len);
            // duplicate waypoints: no direction to take
            Some((len > 0.0).then(|| {
                let dir = d / len;
                let v = p - seg.start;
                let t = (v.x * dir.x + v.y * dir.y).clamp(0.0, len);
                let center = seg.start + dir * t;
                let e = p - center;
                let gate = Gate { center: center.into(), direction: dir, half_width: GATE_HALF_WIDTH, distance_along_track: start + t };
                (e.x.hypot(e.y), gate)
            }))
        })
        .flatten()
}

/// The snap with the smallest offset, the first on ties.
fn nearest(snaps: impl Iterator<Item = (f64, Gate)>) -> Option<(f64, Gate)> {
    snaps.min_by(|a, b| a.0.total_cmp(&b.0))
}

#[derive(thiserror::Error, Debug)]
pub enum TrackBuildError {
    #[error("a track needs at least 2 waypoints")]
    TooFewWaypoints,
    #[error("placement is {distance} from the track, too far to project onto it")]
    PlacementOffTrack { distance: f64 },
    #[error("placement is {overshoot} past the end of the track; extend the waypoints to cover it")]
    PlacementBeyondTrack { overshoot: f64 },
    #[error("finish gate must come after the start gate along the waypoints")]
    FinishBeforeStart,
    #[error("sector gate at {distance_along_track} is not strictly between start and finish")]
    SectorOutsideTrack { distance_along_track: f64 },
}

#[derive(Debug)]
pub struct Projection {
    lon0: f64,
    lat0: f64,
}

impl Projection {
    pub fn new(waypoint: Waypoint) -> Self {
        let lon0 = waypoint.x();
        let lat0 = waypoint.y();
        Self { lon0, lat0 }
    }
    pub fn project(&self, point: Point) -> Point {
        let x = EARTH_RADIUS * (point.x() - self.lon0).to_radians() * self.lat0.to_radians().cos();
        let y = EARTH_RADIUS * (point.y() - self.lat0).to_radians();
        Point::new(x, y)
    }
    pub fn unproject(&self, point: Point) -> Point {
        let lon =
            (point.x() / (EARTH_RADIUS * self.lat0.to_radians().cos())).to_degrees() + self.lon0;
        let lat = (point.y() / EARTH_RADIUS).to_degrees() + self.lat0;
        Point::new(lon, lat)
    }
}

#[derive(Debug, Default)]
pub struct Gate {
    pub center: Point,
    pub direction: Coord<f64>,
    pub half_width: f64,
    pub distance_along_track: f64,
}

impl Gate {
    /// The crossing line: `half_width` either side of `center`, perpendicular to `direction`.
    pub fn segment(&self) -> Line {
        let normal = coord! { x: -self.direction.y, y: self.direction.x } * self.half_width;
        Line::new(self.center.0 - normal, self.center.0 + normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISBON: Waypoint = Point(coord! { x: -9.0, y: 38.7 });
    /// Metres per 0.001° of latitude: R · 0.001° in radians.
    const M_PER_MILLIDEG: f64 = EARTH_RADIUS * 0.001 * std::f64::consts::PI / 180.0;

    #[test]
    fn project_origin_is_zero() {
        assert_eq!(
            Projection::new(LISBON).project(LISBON),
            Point::new(0.0, 0.0)
        );
    }

    #[test]
    fn project_scales_to_metres() {
        let p = Projection::new(LISBON);
        let north = p.project(Point::new(-9.0, 38.701));
        let east = p.project(Point::new(-8.999, 38.7));
        assert!((north.y() - M_PER_MILLIDEG).abs() < 1e-6);
        assert!((east.x() - M_PER_MILLIDEG * 38.7_f64.to_radians().cos()).abs() < 1e-6);
        assert!((north.y() - 111.19).abs() < 0.01 && (east.x() - 86.77).abs() < 0.01);
    }

    #[test]
    fn unproject_inverts_project() {
        let p = Projection::new(LISBON);
        for w in [
            Point::new(-9.0, 38.7),
            Point::new(-8.95, 38.72),
            Point::new(-9.1, 38.65),
        ] {
            let back = p.unproject(p.project(w));
            assert!((back.x() - w.x()).abs() < 1e-9 && (back.y() - w.y()).abs() < 1e-9);
        }
    }

    fn l_shape() -> Polyline {
        Polyline::from(vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)])
    }

    #[test]
    fn place_gate_snaps_onto_first_segment() {
        let g = place_gate(&l_shape(), Point::new(40.0, 5.0)).unwrap();
        assert_eq!(g.center, Point::new(40.0, 0.0));
        assert_eq!(g.direction, coord! { x: 1.0, y: 0.0 });
        assert_eq!(g.distance_along_track, 40.0);
    }

    #[test]
    fn place_gate_accumulates_distance_across_segments() {
        let g = place_gate(&l_shape(), Point::new(105.0, 50.0)).unwrap();
        assert_eq!(g.center, Point::new(100.0, 50.0));
        assert_eq!(g.direction, coord! { x: 0.0, y: 1.0 });
        assert_eq!(g.distance_along_track, 150.0);
    }

    #[test]
    fn place_gate_rejects_far_placement() {
        let err = place_gate(&l_shape(), Point::new(40.0, -500.0)).unwrap_err();
        assert!(
            matches!(err, TrackBuildError::PlacementOffTrack { distance } if distance == 500.0)
        );
    }

    #[test]
    fn place_gate_rejects_placement_before_start() {
        let err = place_gate(&l_shape(), Point::new(-10.0, 0.0)).unwrap_err();
        assert!(matches!(err, TrackBuildError::PlacementBeyondTrack { overshoot } if overshoot == 10.0));
    }

    #[test]
    fn place_gate_rejects_placement_past_finish() {
        let err = place_gate(&l_shape(), Point::new(100.0, 110.0)).unwrap_err();
        assert!(matches!(err, TrackBuildError::PlacementBeyondTrack { overshoot } if overshoot == 10.0));
    }

    #[test]
    fn place_gate_accepts_placement_exactly_on_ends() {
        assert_eq!(place_gate(&l_shape(), Point::new(0.0, 0.0)).unwrap().distance_along_track, 0.0);
        assert_eq!(place_gate(&l_shape(), Point::new(100.0, 100.0)).unwrap().distance_along_track, 200.0);
    }

    #[test]
    fn place_gate_allows_outside_corner_on_closed_loop() {
        let mut square = Polyline::from(vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]);
        square.close();
        let g = place_gate(&square, Point::new(-5.0, -5.0)).unwrap();
        assert_eq!(g.center, Point::new(0.0, 0.0));
    }

    #[test]
    fn place_gate_skips_duplicate_waypoints() {
        let poly = Polyline::from(vec![(0.0, 0.0), (0.0, 0.0), (100.0, 0.0)]);
        let g = place_gate(&poly, Point::new(10.0, 1.0)).unwrap();
        assert_eq!(g.direction, coord! { x: 1.0, y: 0.0 });
        assert_eq!(g.distance_along_track, 10.0);
    }

    /// ~347 m straight east from Lisbon, one waypoint every 0.002° of longitude.
    fn eastbound() -> TrackBuilder {
        let open = TrackShape::Open { start: LISBON, finish: Point::new(-8.996, 38.7) };
        Track::builder("Eastbound", open).waypoints([
            LISBON,
            Point::new(-8.998, 38.7),
            Point::new(-8.996, 38.7),
        ])
    }

    #[test]
    fn build_places_sector_gates_sorted_by_distance() {
        let m_per_millideg_lon = M_PER_MILLIDEG * 38.7_f64.to_radians().cos();
        let track = eastbound()
            .sector_gate_at(Point::new(-8.997, 38.7001)) // added first, sits further along
            .sector_gate_at(Point::new(-8.999, 38.7))
            .build()
            .unwrap();

        let dists: Vec<f64> = track.sector_gates.iter().map(|g| g.distance_along_track).collect();
        assert_eq!(dists.len(), 2);
        assert!((dists[0] - m_per_millideg_lon).abs() < 1e-6);
        assert!((dists[1] - 3.0 * m_per_millideg_lon).abs() < 1e-6);
        assert!(track.sector_gates.iter().all(|g| g.center.y().abs() < 1e-9)); // snapped onto the road
    }

    #[test]
    fn build_fails_when_a_sector_gate_is_off_track() {
        let err = eastbound()
            .sector_gate_at(Point::new(-8.999, 38.7))
            .sector_gate_at(Point::new(-8.998, 38.71)) // ~1.1 km north
            .build()
            .unwrap_err();
        assert!(matches!(err, TrackBuildError::PlacementOffTrack { distance } if distance > 1000.0));
    }

    #[test]
    fn build_places_open_boundary_gates() {
        let track = eastbound().build().unwrap();
        let Boundary::Open { start, finish } = &track.boundary else {
            panic!("expected Open boundary");
        };
        assert_eq!(start.distance_along_track, 0.0);
        assert!((finish.distance_along_track - 4.0 * M_PER_MILLIDEG * 38.7_f64.to_radians().cos()).abs() < 1e-6);
        assert_eq!(start.direction, coord! { x: 1.0, y: 0.0 });
    }

    #[test]
    fn build_places_closed_boundary_gate() {
        let track = Track::builder("Loop", TrackShape::Closed { gate: Point::new(-8.999, 38.7) })
            .waypoints([LISBON, Point::new(-8.998, 38.7), Point::new(-8.998, 38.701)])
            .build()
            .unwrap();
        let Boundary::Closed { gate } = &track.boundary else {
            panic!("expected Closed boundary");
        };
        assert!((gate.distance_along_track - M_PER_MILLIDEG * 38.7_f64.to_radians().cos()).abs() < 1e-6);
    }

    #[test]
    fn build_fails_when_finish_is_off_track() {
        let open = TrackShape::Open { start: LISBON, finish: Point::new(-8.996, 38.71) };
        let err = Track::builder("Bad", open)
            .waypoints([LISBON, Point::new(-8.996, 38.7)])
            .build()
            .unwrap_err();
        assert!(matches!(err, TrackBuildError::PlacementOffTrack { .. }));
    }

    #[test]
    fn build_rejects_single_waypoint() {
        let err = Track::builder("Dot", TrackShape::Closed { gate: LISBON })
            .waypoints([LISBON])
            .build()
            .unwrap_err();
        assert!(matches!(err, TrackBuildError::TooFewWaypoints));
    }

    fn segment_lengths(poly: &Polyline) -> impl Iterator<Item = f64> + '_ {
        poly.lines().map(|l| l.delta().x.hypot(l.delta().y))
    }

    #[test]
    fn test_tracker_builder_build_open_track() {
        let waypoints = vec![LISBON, Point::new(-8.998, 38.7), Point::new(-8.996, 38.7)];
        let open = TrackShape::Open { start: LISBON, finish: waypoints[2] };
        let track = Track::builder("TestOpenTrack", open).waypoints(waypoints.clone()).build().unwrap();

        assert_eq!(track.name, "TestOpenTrack");
        assert_eq!(track.waypoints, waypoints);
        assert!(track.sector_gates.is_empty());

        // Densified: every projected waypoint survives, no segment exceeds the spacing.
        for w in &waypoints {
            assert!(track.polyline.0.contains(&track.projection.project(*w).0));
        }
        assert!(segment_lengths(&track.polyline).all(|l| l <= DENSIFY_SPACING + 1e-9));
        assert_eq!(track.polyline[0], coord! {x: 0.0, y: 0.0});
    }

    #[test]
    fn test_tracker_builder_build_closed_track() {
        let waypoints = vec![LISBON, Point::new(-8.998, 38.7), Point::new(-8.998, 38.701)];
        let closed = TrackShape::Closed { gate: Point::new(-8.999, 38.7) };
        let track = Track::builder("TestClosedTrack", closed).waypoints(waypoints.clone()).build().unwrap();

        assert_eq!(track.name, "TestClosedTrack");
        assert!(track.polyline.is_closed());
        assert!(segment_lengths(&track.polyline).all(|l| l <= DENSIFY_SPACING + 1e-9));
    }

    /// 100 m x 100 m square loop from Lisbon (first leg east), as waypoints.
    fn square_loop() -> Vec<Waypoint> {
        let p = Projection::new(LISBON);
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
            .map(|(x, y)| p.unproject(Point::new(x, y)))
            .to_vec()
    }

    fn at(x: f64, y: f64) -> Waypoint {
        Projection::new(LISBON).unproject(Point::new(x, y))
    }

    #[test]
    fn build_orders_closed_sectors_from_the_gate_across_the_seam() {
        // Gate at 150 m; sectors at 50 m and 250 m should run 250 -> 50 (wrapping past 400 = 0).
        let track = Track::builder("Loop", TrackShape::Closed { gate: at(100.0, 50.0) })
            .waypoints(square_loop())
            .sector_gate_at(at(50.0, 0.0))
            .sector_gate_at(at(50.0, 100.0))
            .build()
            .unwrap();
        let d: Vec<f64> = track.sector_gates.iter().map(|g| g.distance_along_track).collect();
        assert!((d[0] - 250.0).abs() < 1e-6 && (d[1] - 50.0).abs() < 1e-6, "{d:?}");
    }

    #[test]
    fn build_rejects_finish_before_start() {
        let open = TrackShape::Open { start: Point::new(-8.996, 38.7), finish: LISBON };
        let err = Track::builder("Backwards", open)
            .waypoints([LISBON, Point::new(-8.996, 38.7)])
            .build()
            .unwrap_err();
        assert!(matches!(err, TrackBuildError::FinishBeforeStart));
    }

    #[test]
    fn build_rejects_sector_outside_open_track() {
        // Waypoints run past both gates; sector sits before the start.
        let open = TrackShape::Open { start: at(20.0, 0.0), finish: at(80.0, 0.0) };
        let err = Track::builder("Short", open)
            .waypoints([LISBON, at(100.0, 0.0)])
            .sector_gate_at(at(10.0, 0.0))
            .build()
            .unwrap_err();
        assert!(matches!(err, TrackBuildError::SectorOutsideTrack { distance_along_track } if (distance_along_track - 10.0).abs() < 1e-6));
    }

    #[test]
    fn gates_ordered_walks_start_sectors_finish() {
        let track = eastbound().sector_gate_at(Point::new(-8.998, 38.7)).build().unwrap();
        let d: Vec<f64> = track.gates_ordered().map(|g| g.distance_along_track).collect();
        assert_eq!(d.len(), 3);
        assert!(d.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn sectors_sum_to_lap_on_closed_track() {
        let track = Track::builder("Loop", TrackShape::Closed { gate: at(100.0, 50.0) })
            .waypoints(square_loop())
            .sector_gate_at(at(50.0, 0.0))
            .build()
            .unwrap();
        let lengths: Vec<f64> = track.sectors().map(|s| s.length).collect();
        assert!((track.length() - 400.0).abs() < 1e-6);
        assert!((lengths[0] - 300.0).abs() < 1e-6 && (lengths[1] - 100.0).abs() < 1e-6, "{lengths:?}");
    }

    #[test]
    fn open_track_gated_on_its_ends_is_timed_end_to_end() {
        let track = eastbound().build().unwrap();
        let lengths: Vec<f64> = track.sectors().map(|s| s.length).collect();
        assert_eq!(lengths.len(), 1);
        assert!(lengths[0] > 0.0 && close_to(lengths[0], track.length()), "{lengths:?}");
    }

    #[test]
    fn single_sector_on_closed_track_is_a_full_lap() {
        let track = Track::builder("Loop", TrackShape::Closed { gate: at(100.0, 50.0) })
            .waypoints(square_loop())
            .build()
            .unwrap();
        let lengths: Vec<f64> = track.sectors().map(|s| s.length).collect();
        assert_eq!(lengths.len(), 1);
        assert!((lengths[0] - 400.0).abs() < 1e-6);
    }

    fn square() -> Track {
        Track::builder("Loop", TrackShape::Closed { gate: at(50.0, 0.0) }).waypoints(square_loop()).build().unwrap()
    }

    fn close_to(d: f64, want: f64) -> bool {
        (d - want).abs() < 1e-6
    }

    #[test]
    fn locate_measures_distance_along_the_polyline() {
        let t = square();
        assert!(close_to(t.locate(coord! { x: 30.0, y: -3.0 }, None), 30.0));
        assert!(close_to(t.locate(coord! { x: 104.0, y: 60.0 }, None), 160.0));
    }

    #[test]
    fn locate_follows_the_hint_onto_a_parallel_leg() {
        // U-turn: out along y = 0, back along y = 10.
        let open = TrackShape::Open { start: at(10.0, 0.0), finish: at(10.0, 10.0) };
        let t = Track::builder("U", open)
            .waypoints([at(0.0, 0.0), at(100.0, 0.0), at(100.0, 10.0), at(0.0, 10.0)])
            .build()
            .unwrap();
        let p = coord! { x: 50.0, y: 4.0 };
        assert!(close_to(t.locate(p, None), 50.0));
        assert!(close_to(t.locate(p, Some(155.0)), 160.0));
    }

    #[test]
    fn locate_wraps_past_the_seam_of_a_loop() {
        assert!(close_to(square().locate(coord! { x: 5.0, y: 1.0 }, Some(395.0)), 5.0));
        assert!(close_to(square().locate(coord! { x: 1.0, y: 5.0 }, Some(5.0)), 395.0));
    }

    #[test]
    fn locate_searches_everywhere_when_the_hint_is_stale() {
        // After a dropout: nothing near the hint lies near the fix.
        assert!(close_to(square().locate(coord! { x: 100.0, y: 80.0 }, Some(50.0)), 180.0));
    }

    #[test]
    fn gate_segment_is_perpendicular_to_travel() {
        let g = Gate { center: Point::new(10.0, 0.0), direction: coord! {x: 1.0, y: 0.0}, half_width: 15.0, distance_along_track: 10.0 };
        assert_eq!(g.segment(), Line::new(coord! {x: 10.0, y: -15.0}, coord! {x: 10.0, y: 15.0}));
    }
}
