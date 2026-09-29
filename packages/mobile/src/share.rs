//! A track's shareable card: an SVG for the in-app preview, rasterised to a PNG for sharing.

use std::sync::Arc;

use resvg::{tiny_skia::Pixmap, usvg};
use traced_core::track::Track;

use crate::views::track_map::Layout;

const W: f64 = 1080.0;
const H: f64 = 1350.0;
const FONT: &str = "Barlow Semi Condensed";
/// The map's box: x, y, width, height.
const MAP: [f64; 4] = [80.0, 190.0, 920.0, 740.0];
/// `main.css`'s sector colours, identity only.
const SECTORS: [&str; 4] = ["#2563eb", "#0891b2", "#4f46e5", "#64748b"];

/// Portrait (1080×1350) and self-contained: its CSS is scoped to `.traced-card` and
/// the map's strokes are in the map's own units, so it previews inline in the webview too.
/// `stats` are up to three (label, value, accent) columns, already formatted.
pub fn card_svg(track: &Track, stats: &[(String, String, bool)]) -> String {
    let layout = Layout::new(track);
    let [x, y, w, h] = layout.view_box;
    let [mx, my, mw, mh] = MAP;
    // Map units per card pixel, as the map is fitted into its box.
    let px = (w / mw).max(h / mh);
    let map = SECTORS
        .iter()
        .enumerate()
        .fold(layout.markup(None), |m, (k, c)| {
            m.replace(&format!("var(--sector-{k})"), c)
        });
    let name = esc(&track.name);
    let name_size = (1700.0 / track.name.chars().count().max(1) as f64).min(104.0);
    let (_, tags) = track.tags.iter().fold((mx, String::new()), |(x, s), tag| {
        let w = tag.chars().count() as f64 * 15.0 + 44.0;
        let pill = format!(
            r##"<rect x="{x}" y="1072" width="{w}" height="48" rx="24" fill="#292524"/><text x="{}" y="1105" font-size="28" fill="#a8a29e" text-anchor="middle">{}</text>"##,
            x + w / 2.0,
            esc(tag)
        );
        (x + w + 14.0, s + &pill)
    });
    let stats = stats
        .iter()
        .zip([mx, 400.0, 720.0])
        .map(|((label, value, accent), x)| {
            format!(
                r##"<text x="{x}" y="1200" font-size="28" letter-spacing="3" fill="#a8a29e">{}</text><text x="{x}" y="1262" font-size="56" fill="{}">{value}</text>"##,
                esc(&label.to_uppercase()),
                if *accent { "#ea580c" } else { "#fafaf9" }
            )
        })
        .collect::<String>();
    let icon = include_str!("../assets/icon.svg")
        .replace("<svg ", r#"<svg x="56" y="52" width="108" height="108" "#)
        .replace("trail", "traced-card-trail");
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" class="traced-card" viewBox="0 0 {W} {H}" font-family="'{FONT}', sans-serif" font-weight="600">
<style>.traced-card .outline, .traced-card .sector {{ fill: none; stroke-linejoin: round; stroke-linecap: round }}
.traced-card .outline {{ stroke: #fafaf9; stroke-opacity: .12; stroke-width: {o} }}
.traced-card .sector {{ stroke-width: {s} }}
.traced-card .gate {{ stroke: #a8a29e; stroke-linecap: round; stroke-width: {g} }}
.traced-card .gate.boundary {{ stroke: #fafaf9; stroke-width: {b} }}
.traced-card .arrow {{ display: none }}</style>
<rect width="{W}" height="{H}" fill="#111317"/>{icon}
<text x="166" y="124" font-size="52" fill="#ea580c">Traced</text>
<svg x="{mx}" y="{my}" width="{mw}" height="{mh}" viewBox="{x} {y} {w} {h}">{map}</svg>
<text x="{mx}" y="1030" font-size="{name_size:.0}" fill="#fafaf9">{name}</text>{tags}{stats}</svg>"##,
        o = 34.0 * px,
        s = 14.0 * px,
        g = 7.0 * px,
        b = 12.0 * px,
    )
}

/// `svg` (a `card_svg`) as a PNG, in the bundled font: Android has no system fonts resvg can reach.
pub fn card_png(svg: &str) -> Result<Vec<u8>, String> {
    let mut opt = usvg::Options {
        font_family: FONT.into(),
        ..Default::default()
    };
    Arc::make_mut(&mut opt.fontdb).load_font_data(
        include_bytes!("../assets/fonts/BarlowSemiCondensed-SemiBold.ttf").to_vec(),
    );
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(size.width(), size.height()).ok_or("empty card")?;
    resvg::render(&tree, Default::default(), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| e.to_string())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracks::PRESETS;

    #[test]
    fn presets_render_to_png() {
        for (i, p) in PRESETS.iter().enumerate() {
            let track = p.track.clone().build().unwrap();
            let stats = [("Length", "856 m", false), ("Sectors", "3", false), ("Best lap", "0:52.3", true)]
                .map(|(l, v, a)| (l.into(), v.into(), a));
            let png = card_png(&card_svg(&track, &stats[..2 + (i == 0) as usize])).unwrap();
            assert_eq!(png[..8], *b"\x89PNG\r\n\x1a\n");
            // IHDR's width and height, big-endian.
            assert_eq!(png[16..24], [0, 0, 4, 56, 0, 0, 5, 70]);
            if let Ok(dir) = std::env::var("CARD_DIR") {
                std::fs::write(format!("{dir}/card_{i}.png"), png).unwrap();
            }
        }
    }
}
