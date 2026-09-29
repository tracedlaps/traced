//! Waypoints from files other apps export: a CSV with lat/lon columns (e.g. Litchi), GPX or KML;
//! and whole tracks from a GPX `export::gpx` wrote.

use geo::{Distance, Haversine};

use super::{Track, TrackBuilder, TrackShape, Waypoint};

/// A path ending closer than this (m) to its start looks like a loop.
const CLOSED_GAP: f64 = 30.0;

#[derive(thiserror::Error, Debug)]
pub enum ImportError {
    #[error("not a CSV with latitude and longitude columns, a GPX or a KML file")]
    Unrecognised,
    #[error("line {0} has no valid latitude and longitude")]
    BadLine(usize),
    #[error(transparent)]
    Xml(#[from] roxmltree::Error),
}

/// What a file holds: always a path, and whatever of the rest of a track it carries.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Imported {
    pub name: Option<String>,
    pub tags: Vec<String>,
    pub waypoints: Vec<Waypoint>,
    /// From GPX `wpt`s typed `start` (and `finish`, if Open).
    pub shape: Option<TrackShape>,
    /// From GPX `wpt`s typed `sector`, in file order.
    pub sectors: Vec<Waypoint>,
}

impl Imported {
    /// The whole track, if the file places its gates; unnamed if it has no name.
    pub fn builder(&self) -> Option<TrackBuilder> {
        let track = Track::builder(self.name.clone().unwrap_or_default(), self.shape?)
            .tags(self.tags.clone())
            .waypoints(self.waypoints.clone());
        Some(self.sectors.iter().fold(track, |b, &s| b.sector_gate_at(s)))
    }
}

/// The file's track; the format is told from the content.
pub fn track(text: &str) -> Result<Imported, ImportError> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.starts_with('<') {
        xml(text)
    } else {
        csv(text).map(|waypoints| Imported { waypoints, ..Default::default() })
    }
}

/// The file's waypoints in path order.
pub fn waypoints(text: &str) -> Result<Vec<Waypoint>, ImportError> {
    track(text).map(|t| t.waypoints)
}

/// Whether the path ends back near its start, i.e. is likely a Closed track.
pub fn looks_closed(waypoints: &[Waypoint]) -> bool {
    waypoints.len() > 2
        && Haversine.distance(waypoints[0], waypoints[waypoints.len() - 1]) < CLOSED_GAP
}

/// Header row naming the columns; every other non-blank row is a waypoint.
fn csv(text: &str) -> Result<Vec<Waypoint>, ImportError> {
    let mut lines = text.lines().enumerate();
    let (_, header) = lines.next().ok_or(ImportError::Unrecognised)?;
    let column = |names: &[&str]| {
        header
            .split(',')
            .position(|h| names.contains(&h.trim().trim_matches('"').to_ascii_lowercase().as_str()))
    };
    let (lat, lon) = column(&["lat", "latitude"])
        .zip(column(&["lon", "lng", "long", "longitude"]))
        .ok_or(ImportError::Unrecognised)?;
    lines
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            let cols: Vec<_> = line.split(',').collect();
            let num = |c: usize| cols.get(c)?.trim().trim_matches('"').parse().ok();
            num(lon)
                .zip(num(lat))
                .map(|(lon, lat)| Waypoint::new(lon, lat))
                .ok_or(ImportError::BadLine(i + 1))
        })
        .collect()
}

/// GPX: a track's points, else a route's, else loose waypoints; the metadata's name and keywords;
/// and gates from `wpt`s typed `start`, `finish` or `sector`.
/// KML: the first line's coordinates, else every placemark's in order.
fn xml(text: &str) -> Result<Imported, ImportError> {
    let doc = roxmltree::Document::parse(text)?;
    let named = |tag: &'static str| {
        doc.descendants()
            .filter(move |n| n.tag_name().name() == tag)
    };
    fn child<'a>(n: roxmltree::Node<'a, '_>, tag: &str) -> Option<&'a str> {
        n.children().find(|c| c.tag_name().name() == tag)?.text()
    }
    let point = |n: roxmltree::Node| {
        Some(Waypoint::new(
            n.attribute("lon")?.parse().ok()?,
            n.attribute("lat")?.parse().ok()?,
        ))
    };
    let typed = |kind| named("wpt").filter(move |&n| child(n, "type") == Some(kind)).filter_map(point);
    for tag in ["trkpt", "rtept", "wpt"] {
        let waypoints: Vec<_> = named(tag).filter_map(point).collect();
        if !waypoints.is_empty() {
            let metadata = |tag| named("metadata").next().and_then(|m| child(m, tag));
            let shape = match (typed("start").next(), typed("finish").next()) {
                (Some(start), Some(finish)) => Some(TrackShape::Open { start, finish }),
                (Some(gate), None) => Some(TrackShape::Closed { gate }),
                _ => None,
            };
            let tags = metadata("keywords").unwrap_or_default().split(',').map(str::trim);
            return Ok(Imported {
                name: metadata("name").map(Into::into),
                tags: tags.filter(|t| !t.is_empty()).map(Into::into).collect(),
                waypoints,
                shape,
                sectors: typed("sector").collect(),
            });
        }
    }
    let mut coords: Vec<_> = named("coordinates").collect();
    if let Some(line) = coords.iter().find(|n| {
        n.parent()
            .is_some_and(|p| p.tag_name().name() == "LineString")
    }) {
        coords = vec![*line];
    }
    let points = coords
        .iter()
        .flat_map(|n| n.text().unwrap_or_default().split_whitespace())
        .map(|tuple| {
            let mut c = tuple.split(',').map(str::parse);
            match (c.next(), c.next()) {
                (Some(Ok(lon)), Some(Ok(lat))) => Ok(Waypoint::new(lon, lat)),
                _ => Err(ImportError::Unrecognised),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if points.is_empty() {
        Err(ImportError::Unrecognised)
    } else {
        Ok(Imported { waypoints: points, ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lon_lat(w: &[Waypoint]) -> Vec<(f64, f64)> {
        w.iter().map(|p| (p.x(), p.y())).collect()
    }

    #[test]
    fn reads_litchi_csv() {
        let csv = "lat,lon,alt,speed\n49.81,6.21,50,10\n49.82,6.22,50,10\n\n";
        assert_eq!(
            lon_lat(&waypoints(csv).unwrap()),
            [(6.21, 49.81), (6.22, 49.82)]
        );
    }

    #[test]
    fn reads_csv_with_other_column_names() {
        let csv = "\u{feff}name,\"Longitude\",\"Latitude\"\nA,6.21,49.81\nB,6.22,49.82";
        assert_eq!(
            lon_lat(&waypoints(csv).unwrap()),
            [(6.21, 49.81), (6.22, 49.82)]
        );
    }

    #[test]
    fn rejects_bad_csv() {
        assert!(matches!(
            waypoints("x,y\n1,2"),
            Err(ImportError::Unrecognised)
        ));
        assert!(matches!(
            waypoints("lat,lon\n1,2\n3,oops"),
            Err(ImportError::BadLine(3))
        ));
    }

    #[test]
    fn reads_gpx_track_over_waypoints() {
        let gpx = r#"<?xml version="1.0"?>
            <gpx xmlns="http://www.topografix.com/GPX/1/1">
              <wpt lat="1" lon="1"/>
              <trk><trkseg><trkpt lat="49.81" lon="6.21"/><trkpt lat="49.82" lon="6.22"/></trkseg></trk>
            </gpx>"#;
        assert_eq!(
            lon_lat(&waypoints(gpx).unwrap()),
            [(6.21, 49.81), (6.22, 49.82)]
        );
    }

    #[test]
    fn reads_gpx_gates_from_typed_waypoints() {
        let gpx = r#"<gpx><metadata><name>A &amp; B</name><keywords>karting, indoor</keywords></metadata>
              <wpt lat="49.81" lon="6.21"><type>start</type></wpt>
              <wpt lat="49.82" lon="6.22"><type>sector</type></wpt>
              <wpt lat="49.83" lon="6.23"/>
              <trk><trkseg><trkpt lat="49.81" lon="6.21"/><trkpt lat="49.82" lon="6.22"/></trkseg></trk>
            </gpx>"#;
        let t = track(gpx).unwrap();
        assert_eq!(t.name.as_deref(), Some("A & B"));
        assert_eq!(t.tags, ["karting", "indoor"]);
        assert_eq!(t.shape, Some(TrackShape::Closed { gate: Waypoint::new(6.21, 49.81) }));
        assert_eq!(lon_lat(&t.sectors), [(6.22, 49.82)]);
        assert_eq!(lon_lat(&t.waypoints), [(6.21, 49.81), (6.22, 49.82)]);
    }

    #[test]
    fn other_files_carry_no_gates() {
        let untyped = r#"<gpx><wpt lat="49.81" lon="6.21"/><wpt lat="49.82" lon="6.22"/></gpx>"#;
        for text in [untyped, "lat,lon\n49.81,6.21\n49.82,6.22"] {
            let t = track(text).unwrap();
            assert!(t.shape.is_none() && t.builder().is_none());
            assert_eq!(lon_lat(&t.waypoints), [(6.21, 49.81), (6.22, 49.82)]);
        }
    }

    #[test]
    fn reads_kml_line_over_placemarks() {
        let kml = r#"<kml xmlns="http://www.opengis.net/kml/2.2"><Document>
            <Placemark><Point><coordinates>1,1,0</coordinates></Point></Placemark>
            <Placemark><LineString><coordinates>
              6.21,49.81,0 6.22,49.82,0
            </coordinates></LineString></Placemark>
            </Document></kml>"#;
        assert_eq!(
            lon_lat(&waypoints(kml).unwrap()),
            [(6.21, 49.81), (6.22, 49.82)]
        );
        assert!(matches!(
            waypoints("<kml/>"),
            Err(ImportError::Unrecognised)
        ));
    }

    #[test]
    fn loops_look_closed() {
        let p = |lat| Waypoint::new(6.21, lat);
        assert!(looks_closed(&[p(49.81), p(49.82), p(49.8101)]));
        assert!(!looks_closed(&[p(49.81), p(49.82), p(49.83)]));
    }
}
