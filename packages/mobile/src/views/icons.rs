//! The app's 24×24 stroke icons, as SVG path data, and the component that draws them.

use dioxus::prelude::*;

pub const HISTORY: &str = "M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5M12 7v5l3 3";
pub const FLAG: &str = "M5 21V4M5 4h11l-2 4 2 4H5";
pub const SPEEDO: &str = "M4.5 17a9 9 0 1 1 15 0M12 13l4-5";
pub const PENCIL: &str = "M4 20h4L19 9l-4-4L4 16zM13 7l4 4";
pub const SLIDERS: &str = "M21 4h-7M10 4H3M21 12h-9M8 12H3M21 20h-5M12 20H3M14 2v4M8 10v4M16 18v4";
pub const PIN: &str =
    "M12 21s-7-6.2-7-12a7 7 0 0 1 14 0c0 5.8-7 12-7 12zM12 6.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5";
pub const CLOCK: &str = "M3 12a9 9 0 1 0 18 0 9 9 0 0 0-18 0M12 7v5l3 3";
pub const PHONE: &str =
    "M8 2h8a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2M11 18h2";
pub const BELL: &str = "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10.3 21a1.9 1.9 0 0 0 3.4 0";
pub const BACK: &str = "M9 14 4 9l5-5M4 9h11a5 5 0 0 1 0 10h-3";
pub const WARNING: &str =
    "M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0M12 9v4M12 17h.01";
pub const NO_ENTRY: &str = "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18M5.6 5.6l12.8 12.8";
pub const SHIELD: &str = "M12 2l8 3v6c0 5-3.5 9-8 11-4.5-2-8-6-8-11V5z";

/// One of the icons above, coloured by the surrounding CSS.
#[component]
pub fn Icon(d: &'static str) -> Element {
    rsx! { svg { view_box: "0 0 24 24", path { d } } }
}
