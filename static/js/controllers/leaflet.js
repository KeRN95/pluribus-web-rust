// Not a controller: Leaflet, which draws the maps (map_controller.js, listing_map_controller.js),
// and the tiles they draw: OpenStreetMap's.

const LEAFLET = "/static/vendor/leaflet-1.9.4";

// Leaflet is only fetched on pages with a map, and only once: its script and its stylesheet.
let leaflet;
export function loadLeaflet() {
  leaflet ??= new Promise((resolve, reject) => {
    const style = Object.assign(document.createElement("link"), { rel: "stylesheet", href: `${LEAFLET}/leaflet.css` });
    const script = Object.assign(document.createElement("script"), { src: `${LEAFLET}/leaflet.js` });
    let waiting = 2;
    const loaded = () => { if (--waiting === 0) resolve(window.L); };
    const failed = () => { leaflet = undefined; reject(new Error("Leaflet did not load")); };
    style.onload = script.onload = loaded;
    style.onerror = script.onerror = failed;
    document.head.append(style, script);
  });
  return leaflet;
}

// OpenStreetMap's tiles, with the credit they ask for.
export function tiles(L) {
  return L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
    maxZoom: 19,
    attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
  });
}
