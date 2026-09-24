use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::stream::{self, BoxStream, StreamExt};
use rand::RngExt;

use crate::gps::{GPSFix, GpsProvider, GpsStatus};

#[derive(Debug)]
pub struct MockGPSConfig {
    pub interval: Duration,
    pub loop_fixes: bool,
}

impl Default for MockGPSConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            loop_fixes: true,
        }
    }
}

pub struct MockGPSProvider {
    config: MockGPSConfig,
    fixes: Vec<GPSFix>,
    last_fix: Arc<Mutex<GPSFix>>,
    status: Arc<Mutex<GpsStatus>>,
}

impl GpsProvider for MockGPSProvider {
    fn init(&mut self) -> Result<(), super::GpsError> {
        tracing::info!("Initializing MockGPSProvider");
        *self.status.lock().unwrap() = GpsStatus::Searching;
        Ok(())
    }

    fn deinit(&self) {
        tracing::info!("De-initializing MockGPSProvider");
    }

    fn stream_fixes(&self) -> BoxStream<'static, Result<super::GPSFix, super::GpsError>> {
        let interval = self.config.interval;
        let fixes = self.fixes.clone();
        let last_fix = self.last_fix.clone();
        let status = self.status.clone();

        let iter: Box<dyn Iterator<Item = GPSFix> + Send> = if self.config.loop_fixes {
            Box::new(fixes.into_iter().cycle())
        } else {
            Box::new(fixes.into_iter())
        };

        stream::iter(iter)
            .then(move |fix| async move {
                tokio::time::sleep(interval).await;
                Ok(fix)
            })
            .inspect(move |result| {
                if let Ok(fix) = result {
                    *last_fix.lock().unwrap() = *fix;
                    *status.lock().unwrap() = GpsStatus::Fixed {
                        satellites: 8,
                        hdop: 1.0,
                    };
                }
            })
            .boxed()
    }

    fn get_fix(&self) -> Option<super::GPSFix> {
        Some(*self.last_fix.lock().unwrap())
    }

    fn status(&self) -> GpsStatus {
        *self.status.lock().unwrap()
    }
}

impl MockGPSProvider {
    pub fn new(fixes: Vec<GPSFix>, config: MockGPSConfig) -> Self {
        Self {
            config,
            fixes,
            last_fix: Arc::new(Mutex::new(GPSFix::default())),
            status: Arc::new(Mutex::new(GpsStatus::default())),
        }
    }

    pub fn random(samples: u8, config: MockGPSConfig) -> Self {
        let mut rng = rand::rng();
        let fixes = (0..samples)
            .map(|_| GPSFix {
                latitude: rng.random_range(-90.0..=90.0),
                longitude: rng.random_range(-180.0..=180.0),
                speed_ms: rng.random_range(0.0..30.0),
                altitude_m: Some(rng.random_range(0.0..3000.0)),
                heading_deg: Some(rng.random_range(0.0..360.0)),
                accuracy: rng.random_range(1.0..20.0),
                timestamp: chrono::Utc::now(),
            })
            .collect();
        Self::new(fixes, config)
    }
}

impl Default for MockGPSProvider {
    fn default() -> Self {
        Self::random(10, MockGPSConfig::default())
    }
}
