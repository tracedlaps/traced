use dioxus::prelude::*;

use crate::prefs::Choice;

/// How speeds and distances are shown. Everything is stored and computed in SI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Units {
    Metric,
    Imperial,
}

impl Choice for Units {
    const KEY: &'static str = "units";
    const OPTIONS: &'static [(Self, &'static str)] = &[(Self::Metric, "metric"), (Self::Imperial, "imperial")];
}

impl Units {
    /// The chosen units, subscribing the calling component to changes.
    pub fn current() -> Self {
        consume_context::<Signal<Units>>()()
    }

    /// Whole km/h or mph.
    pub fn speed_value(self, ms: impl Into<f64>) -> String {
        format!(
            "{:.0}",
            ms.into()
                * if self == Self::Imperial {
                    2.236_936
                } else {
                    3.6
                }
        )
    }

    pub fn speed_unit(self) -> &'static str {
        if self == Self::Imperial {
            "mph"
        } else {
            "km/h"
        }
    }

    pub fn speed(self, ms: impl Into<f64>) -> String {
        format!("{} {}", self.speed_value(ms), self.speed_unit())
    }

    /// A drive's distance: km or mi, to 2 places.
    pub fn distance(self, m: impl Into<f64>) -> String {
        let m = m.into();
        match self {
            Self::Metric => format!("{:.2} km", m / 1000.0),
            Self::Imperial => format!("{:.2} mi", m / 1609.344),
        }
    }

    /// A track's or gate's length: whole m or ft.
    pub fn length(self, m: impl Into<f64>) -> String {
        let m = m.into();
        match self {
            Self::Metric => format!("{m:.0} m"),
            Self::Imperial => format!("{:.0} ft", m * 3.280_84),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Units::*;

    #[test]
    fn converts_from_si() {
        assert_eq!(Metric.speed(20.0), "72 km/h");
        assert_eq!(Imperial.speed(20.0), "45 mph");
        assert_eq!(Metric.distance(1609.344), "1.61 km");
        assert_eq!(Imperial.distance(1609.344), "1.00 mi");
        assert_eq!(Metric.length(100.0), "100 m");
        assert_eq!(Imperial.length(100.0), "328 ft");
    }
}
