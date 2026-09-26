//! JSON contract with the native GPS plugins. Replies are
//! `{"ok":true,"data":…}` or `{"ok":false,"error":"<code>","detail":"…"}`.

use chrono::DateTime;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use traced_core::gps::{FixSource, GPSFix, GpsError};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOpts {
    pub interval_ms: u64,
    pub force_location_manager: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceDto {
    Fused,
    LocationManager,
}

impl From<SourceDto> for FixSource {
    fn from(s: SourceDto) -> Self {
        match s {
            SourceDto::Fused => FixSource::Fused,
            SourceDto::LocationManager => FixSource::LocationManager,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PermissionDto {
    pub granted: bool,
    /// Only a request can tell: denied for good, so only Settings can grant it.
    #[serde(default)]
    pub blocked: bool,
}

/// The location permission as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Granted,
    /// Not granted (yet); the next request shows the dialog.
    Denied,
    /// Denied for good: the dialog won't show again, only Settings can grant it.
    Blocked,
}

impl From<PermissionDto> for Permission {
    fn from(p: PermissionDto) -> Self {
        match (p.granted, p.blocked) {
            (true, _) => Permission::Granted,
            (false, false) => Permission::Denied,
            (false, true) => Permission::Blocked,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct StartedDto {
    pub source: SourceDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDto {
    pub location_enabled: bool,
    pub satellites_used: u8,
    pub source: Option<SourceDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixDto {
    pub lat: f64,
    pub lon: f64,
    pub accuracy_m: f32,
    pub altitude_m: Option<f32>,
    pub vertical_accuracy_m: Option<f32>,
    pub speed_mps: Option<f32>,
    pub speed_accuracy_mps: Option<f32>,
    pub bearing_deg: Option<f32>,
    pub bearing_accuracy_deg: Option<f32>,
    /// Epoch ms derived from the monotonic clock, so it never jumps.
    pub time_ms: i64,
    pub elapsed_realtime_ns: i64,
    pub is_mock: bool,
    pub source: SourceDto,
}

impl From<FixDto> for GPSFix {
    fn from(f: FixDto) -> Self {
        GPSFix {
            latitude: f.lat,
            longitude: f.lon,
            speed_ms: f.speed_mps.unwrap_or(0.0),
            altitude_m: f.altitude_m,
            heading_deg: f.bearing_deg,
            accuracy: f.accuracy_m,
            vertical_accuracy_m: f.vertical_accuracy_m,
            speed_accuracy_ms: f.speed_accuracy_mps,
            heading_accuracy_deg: f.bearing_accuracy_deg,
            timestamp: DateTime::from_timestamp_millis(f.time_ms).unwrap_or_default(),
        }
    }
}

#[derive(Deserialize)]
struct Reply {
    ok: bool,
    #[serde(default)]
    data: Value,
    error: Option<String>,
    #[serde(default)]
    detail: String,
}

/// Decodes a plugin reply; `reply` is the raw ffi result, whose `Err` is a JNI failure.
pub fn invoke<T: DeserializeOwned>(reply: Result<String, String>) -> Result<T, GpsError> {
    let malformed =
        |e: serde_json::Error| GpsError::Unavailable(format!("malformed plugin reply: {e}"));
    let reply: Reply =
        serde_json::from_str(&reply.map_err(GpsError::Unavailable)?).map_err(malformed)?;
    if !reply.ok {
        return Err(match reply.error.as_deref() {
            Some("permission_denied") => GpsError::PermissionDenied,
            Some("location_disabled") => GpsError::LocationDisabled,
            _ => GpsError::Unavailable(reply.detail),
        });
    }
    T::deserialize(reply.data).map_err(malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(data: &str) -> Result<String, String> {
        Ok(format!(r#"{{"ok":true,"data":{data}}}"#))
    }

    fn err(code: &str) -> Result<String, String> {
        Ok(format!(r#"{{"ok":false,"error":"{code}","detail":"why"}}"#))
    }

    #[test]
    fn parses_full_fix() {
        let json = r#"[{"lat":38.7,"lon":-9.1,"accuracyM":3.5,"altitudeM":120.0,"verticalAccuracyM":4.0,
            "speedMps":12.5,"speedAccuracyMps":0.5,"bearingDeg":90.0,"bearingAccuracyDeg":2.0,
            "timeMs":1700000000123,"elapsedRealtimeNs":5000000000,"isMock":false,"source":"fused"}]"#;
        let [dto]: [FixDto; 1] = invoke(ok(json)).unwrap();
        assert_eq!(
            (dto.is_mock, dto.source, dto.elapsed_realtime_ns),
            (false, SourceDto::Fused, 5_000_000_000)
        );
        let fix = GPSFix::from(dto);
        assert_eq!(
            (fix.latitude, fix.longitude, fix.accuracy),
            (38.7, -9.1, 3.5)
        );
        assert_eq!((fix.speed_ms, fix.speed_accuracy_ms), (12.5, Some(0.5)));
        assert_eq!(
            (fix.altitude_m, fix.vertical_accuracy_m),
            (Some(120.0), Some(4.0))
        );
        assert_eq!(
            (fix.heading_deg, fix.heading_accuracy_deg),
            (Some(90.0), Some(2.0))
        );
        assert_eq!(fix.timestamp.timestamp_millis(), 1_700_000_000_123);
    }

    #[test]
    fn parses_fix_with_nulls() {
        let json = r#"[{"lat":0.0,"lon":0.0,"accuracyM":20.0,"altitudeM":null,"verticalAccuracyM":null,
            "speedMps":null,"speedAccuracyMps":null,"bearingDeg":null,"bearingAccuracyDeg":null,
            "timeMs":0,"elapsedRealtimeNs":0,"isMock":true,"source":"location_manager"}]"#;
        let [dto]: [FixDto; 1] = invoke(ok(json)).unwrap();
        assert_eq!(dto.source, SourceDto::LocationManager);
        let fix = GPSFix::from(dto);
        assert_eq!(fix.speed_ms, 0.0);
        assert!([
            fix.altitude_m,
            fix.vertical_accuracy_m,
            fix.speed_accuracy_ms,
            fix.heading_deg,
            fix.heading_accuracy_deg
        ]
        .iter()
        .all(Option::is_none));
    }

    #[test]
    fn parses_other_replies() {
        assert!(invoke::<Vec<FixDto>>(ok("[]")).unwrap().is_empty());
        let permission = |json| Permission::from(invoke::<PermissionDto>(ok(json)).unwrap());
        assert_eq!(permission(r#"{"granted":true}"#), Permission::Granted);
        assert_eq!(permission(r#"{"granted":false}"#), Permission::Denied);
        assert_eq!(
            permission(r#"{"granted":false,"blocked":true}"#),
            Permission::Blocked
        );
        assert_eq!(
            invoke::<StartedDto>(ok(r#"{"source":"fused"}"#))
                .unwrap()
                .source,
            SourceDto::Fused
        );
        let status: StatusDto = invoke(ok(
            r#"{"locationEnabled":true,"satellitesUsed":9,"source":null}"#,
        ))
        .unwrap();
        assert_eq!(
            (
                status.location_enabled,
                status.satellites_used,
                status.source
            ),
            (true, 9, None)
        );
        invoke::<()>(ok("null")).unwrap();
        assert_eq!(
            serde_json::to_string(&StartOpts {
                interval_ms: 100,
                force_location_manager: true
            })
            .unwrap(),
            r#"{"intervalMs":100,"forceLocationManager":true}"#
        );
    }

    #[test]
    fn maps_error_codes() {
        assert!(matches!(
            invoke::<()>(err("permission_denied")),
            Err(GpsError::PermissionDenied)
        ));
        assert!(matches!(
            invoke::<()>(err("location_disabled")),
            Err(GpsError::LocationDisabled)
        ));
        assert!(
            matches!(invoke::<()>(err("unavailable")), Err(GpsError::Unavailable(d)) if d == "why")
        );
        assert!(
            matches!(invoke::<()>(err("something_new")), Err(GpsError::Unavailable(d)) if d == "why")
        );
    }

    #[test]
    fn broken_replies_are_unavailable() {
        assert!(
            matches!(invoke::<()>(Err("JNI call failed".into())), Err(GpsError::Unavailable(d)) if d == "JNI call failed")
        );
        assert!(matches!(
            invoke::<()>(Ok("not json".into())),
            Err(GpsError::Unavailable(_))
        ));
        assert!(matches!(
            invoke::<PermissionDto>(ok("{}")),
            Err(GpsError::Unavailable(_))
        ));
        assert!(matches!(
            invoke::<PermissionDto>(Ok(r#"{"ok":true}"#.into())),
            Err(GpsError::Unavailable(_))
        ));
    }
}
