//! Waypoints from files other apps export: a CSV with lat/lon columns (e.g. Litchi), GPX or KML.

use geo::{Distance, Haversine};

use super::Waypoint;

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

/// The file's waypoints in path order; the format is told from the content.
pub fn waypoints(text: &str) -> Result<Vec<Waypoint>, ImportError> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.starts_with('<') {
        xml(text)
    } else {
        csv(text)
    }
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

/// GPX: a track's points, else a route's, else loose waypoints.
/// KML: the first line's coordinates, else every placemark's in order.
fn xml(text: &str) -> Result<Vec<Waypoint>, ImportError> {
    let doc = roxmltree::Document::parse(text)?;
    let named = |tag: &'static str| {
        doc.descendants()
            .filter(move |n| n.tag_name().name() == tag)
    };
    for tag in ["trkpt", "rtept", "wpt"] {
        let points: Vec<_> = named(tag)
            .filter_map(|n| {
                Some(Waypoint::new(
                    n.attribute("lon")?.parse().ok()?,
                    n.attribute("lat")?.parse().ok()?,
                ))
            })
            .collect();
        if !points.is_empty() {
            return Ok(points);
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
        Ok(points)
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
