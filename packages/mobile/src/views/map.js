// The map's Leaflet side: draws what Rust sends, reports what the user does.
// Appended to the vendored leaflet.js, which only runs the first time (`window.L` guard).
// `eval` closes the channel when this returns, so everything stays on the recv loop below.

const el = document.getElementById('map');
// No double-tap zoom: its two taps would add two waypoints.
const map = L.map(el, { zoomControl: false, doubleClickZoom: false }).setView([30, 0], 2);
const layers = {
    satellite: L.tileLayer(
        'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}',
        { attribution: 'Tiles &copy; Esri', maxZoom: 19 },
    ),
    streets: L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
        attribution: '&copy; OpenStreetMap contributors',
        maxZoom: 19,
    }),
};
let layer = layers.satellite.addTo(map);
// The sheet below it changes its size; Leaflet only measures on init.
new ResizeObserver(() => map.invalidateSize()).observe(el);

const ll = ({ lat, lng }) => ({ lat, lng });
const icon = (className) => L.divIcon({ className, iconSize: [36, 36] });
map.on('click', (e) => dioxus.send({ tap_map: ll(e.latlng) }));

let markers = [], midpoints = [], overlay = null, first = true;
// The traced path, the phone's position, and whether the map follows it (until panned).
let trace = null, me = null, follow = true;
map.on('dragstart', () => (follow = false));

// Handles to insert a waypoint, halfway along each segment long enough on screen to hold one.
let shown = null;
function drawMidpoints() {
    midpoints.forEach((m) => m.remove());
    midpoints = [];
    const pts = shown?.midpoints ? shown.points : [];
    const n = pts.length;
    for (let i = 0; i < (shown?.closed && n > 2 ? n : n - 1); i++) {
        const [a, b] = [pts[i], pts[(i + 1) % n]];
        if (map.latLngToLayerPoint(a).distanceTo(map.latLngToLayerPoint(b)) < 64) continue;
        const m = L.marker([(a.lat + b.lat) / 2, (a.lng + b.lng) / 2], {
            draggable: true,
            icon: L.divIcon({ className: 'midpoint', iconSize: [28, 28] }),
        }).addTo(map);
        const insert = () => dioxus.send({ insert: [i, ll(m.getLatLng())] });
        m.on('click', insert).on('dragend', insert);
        midpoints.push(m);
    }
}
map.on('zoomend', drawMidpoints);

function fit() {
    if (markers.length) map.fitBounds(L.featureGroup(markers).getBounds(), { padding: [48, 48], maxZoom: 18 });
}

// Markers are kept and updated in place, so one isn't rebuilt under a finger.
function draw(view) {
    view.points.forEach((p, i) => {
        let m = markers[i];
        if (!m) {
            m = markers[i] = L.marker(p, { draggable: true, icon: icon(view.classes[i]) }).addTo(map);
            m.on('click', () => dioxus.send({ tap_marker: i }));
            m.on('dragend', () => dioxus.send({ drag: [i, ll(m.getLatLng())] }));
        }
        m.setLatLng(p);
        if (m.options.icon.options.className !== view.classes[i]) m.setIcon(icon(view.classes[i]));
    });
    markers.splice(view.points.length).forEach((m) => m.remove());

    shown = view;
    drawMidpoints();

    overlay?.remove();
    overlay = null;
    if (view.overlay) {
        const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        svg.setAttribute('viewBox', view.overlay.view_box);
        svg.setAttribute('preserveAspectRatio', 'none');
        svg.setAttribute('class', 'track-overlay');
        svg.innerHTML = view.overlay.svg;
        overlay = L.svgOverlay(svg, view.overlay.bounds).addTo(map);
    }

    if (view.trace.length) (trace ??= L.polyline([], { className: 'trace', interactive: false }).addTo(map)).setLatLngs(view.trace);
    else {
        trace?.remove();
        trace = null;
    }
    if (view.me) {
        (me ??= L.marker(view.me, { icon: icon('me'), interactive: false, keyboard: false }).addTo(map)).setLatLng(view.me);
        if (follow) map.getZoom() < 15 ? map.setView(view.me, 17) : map.panTo(view.me);
    } else {
        me?.remove();
        me = null;
    }

    if (first) fit();
    first = false;
}

async function search(q) {
    try {
        const url = `https://nominatim.openstreetmap.org/search?format=json&limit=5&q=${encodeURIComponent(q)}`;
        const hits = await (await fetch(url)).json();
        dioxus.send({ results: hits.map((h) => ({ name: h.display_name, lat: +h.lat, lng: +h.lon })) });
    } catch (e) {
        dioxus.send({ search_failed: String(e) });
    }
}

while (true) {
    const msg = await dioxus.recv();
    // A bare string, whose `.search` would be String.prototype.search.
    if (msg === 'fit') {
        fit();
        continue;
    }
    if (msg.view) draw(msg.view);
    if (msg.search) await search(msg.search);
    if (msg.fly) {
        follow = true;
        map.flyTo([msg.fly.lat, msg.fly.lng], 17);
    }
    if (msg.layer) {
        layer.remove();
        layer = layers[msg.layer].addTo(map).bringToBack();
    }
}
