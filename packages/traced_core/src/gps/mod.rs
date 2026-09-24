use std::fmt::{self, Display};

use chrono::{DateTime, Utc};
use futures::stream::BoxStream;

pub mod mock;

#[derive(Debug, Default, Clone, Copy)]
pub struct GPSFix {
    pub latitude: f64,
    pub longitude: f64,
    pub speed_ms: f32,
    pub altitude_m: Option<f32>,
    pub heading_deg: Option<f32>,
    pub accuracy: f32,
    pub timestamp: DateTime<Utc>,
}

impl Display for GPSFix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {:.6},{:.6} spd={:.1}m/s acc=±{:.0}m",
            self.timestamp.format("%H:%M:%S"),
            self.latitude,
            self.longitude,
            self.speed_ms,
            self.accuracy,
        )?;
        if let Some(alt) = self.altitude_m {
            write!(f, " alt={alt:.0}m")?;
        }
        if let Some(hdg) = self.heading_deg {
            write!(f, " hdg={hdg:.0}°")?;
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum GpsStatus {
    #[default]
    Disconnected,
    Connecting,
    Searching,
    Fixed {
        satellites: u8,
        hdop: f32,
    },
}

#[derive(thiserror::Error, Debug)]
pub enum GpsError {
    #[error("GPS provider is not available: {0}")]
    Unavailable(String),
    #[error("location permission denied")]
    PermissionDenied,
    #[error("device not found")]
    DeviceNotFound,
}

pub trait GpsProvider: Send + Sync {
    fn init(&mut self) -> Result<(), GpsError>;
    fn deinit(&self);
    fn stream_fixes(&self) -> BoxStream<'static, Result<GPSFix, GpsError>>;
    fn get_fix(&self) -> Option<GPSFix>;
    fn status(&self) -> GpsStatus;
}
