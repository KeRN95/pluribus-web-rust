import { Controller } from "@hotwired/stimulus";

// A map of the results: a tap on the map drops the pin (it can be dragged after), the slider
// sets how far around it to look, and the form sends both. On the map page (/harta) that reloads
// only the results panel; on a category or search page with its map open (harta=1) it reloads
// the page, which keeps the category, the filters and the map. Each result gets a dot on the
// map. Leaflet draws the map with OpenStreetMap's tiles.

const LEAFLET = "/static/vendor/leaflet-1.9.4";
const ALBANIA = { center: [41.15, 20.17], zoom: 7 };
// From this zoom on (streets readable) results show one by one and open on a tap; further out
// they are grouped by squares of the screen, CELL pixels a side, and taps go to the map.
const CLOSE_ZOOM = 15;
const CELL = 40;
// The distance slider goes from 0 to 100 and stands for 1 to MAX_RADIUS_KM km, slowly near the
// pin and faster further out: km = MAX_RADIUS_KM ^ (position / 100). As in filters.rs.
const MAX_RADIUS_KM = 50;
// Whether the map card on category pages is folded, kept while the visitor moves around the site.
const FOLDED = "map-folded";
const PIN = '<svg viewBox="0 0 24 24" width="34" height="34" aria-hidden="true"><path fill="currentColor" fill-rule="evenodd" d="M12 2a7 7 0 0 0-7 7c0 5.25 7 13 7 13s7-7.75 7-13a7 7 0 0 0-7-7Zm0 9.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z"/></svg>';

// Leaflet is only fetched on pages with a map, and only once: its script and its stylesheet.
let leaflet;
function loadLeaflet() {
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

export default class extends Controller {
  static targets = [
    "canvas", "form", "latitude", "longitude", "radius", "slider", "radiusLabel", "status", "results", "points", "toggle",
  ];
  // remember: the card folds the same way on the next page (category and search pages).
  static values = { latitude: Number, longitude: Number, remember: Boolean };

  async connect() {
    if (this.rememberValue && remembered(FOLDED)) this.fold(true);
    let L;
    try {
      L = await loadLeaflet();
    } catch {
      this.statusTarget.textContent = "Harta nuk u ngarkua. Provo përsëri.";
      return;
    }
    if (!this.element.isConnected) return;
    this.L = L;

    // A page restored from Turbo's cache still holds the old map's markup.
    this.canvasTarget.replaceChildren();
    this.map = L.map(this.canvasTarget);
    L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
      maxZoom: 19,
      attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
    }).addTo(this.map);
    this.dots = L.layerGroup().addTo(this.map);
    this.map.on("click", (event) => this.place(event.latlng));
    this.map.on("zoomend", () => this.drawPoints());

    if (this.hasLatitudeValue && this.hasLongitudeValue) {
      this.showPin(L.latLng(this.latitudeValue, this.longitudeValue));
      this.fitCircle();
    } else {
      this.map.setView(ALBANIA.center, ALBANIA.zoom);
    }
    this.plot();
  }

  disconnect() {
    this.map?.remove();
    this.map = this.pin = this.circle = null;
  }

  // The pin goes where the map was tapped, the map centres on its circle (on a touch screen it no
  // longer pans by hand, see letPageScroll), and the results follow.
  place(latlng) {
    this.showPin(latlng);
    this.latitudeTarget.value = latlng.lat.toFixed(5);
    this.longitudeTarget.value = latlng.lng.toFixed(5);
    // Kept on the element too, so a page Turbo caches shows this pin when it comes back.
    this.latitudeValue = latlng.lat;
    this.longitudeValue = latlng.lng;
    this.fitCircle();
    this.submit();
  }

  showPin(latlng) {
    const radius = this.radiusMeters;
    if (this.pin) {
      this.pin.setLatLng(latlng);
      this.circle.setLatLng(latlng).setRadius(radius);
      return;
    }
    const icon = this.L.divIcon({ className: "map-pin", html: PIN, iconSize: [34, 34], iconAnchor: [17, 32] });
    // Above the result bubbles, which share its colour in the dark theme.
    this.pin = this.L.marker(latlng, { icon, draggable: true, keyboard: false, zIndexOffset: 1000 }).addTo(this.map);
    this.pin.on("dragend", () => this.place(this.pin.getLatLng()));
    // The colours of the circle and the dots come from app.css, so they follow the theme.
    this.circle = this.L.circle(latlng, {
      radius, className: "map-area", weight: 2, fillOpacity: 0.08, interactive: false,
    }).addTo(this.map);
    this.letPageScroll();
  }

  // With the pin down, the visitor is after the results below the map, so the mouse wheel, and on
  // touch screens a swipe, scroll the page instead of moving the map. A tap or a drag still moves
  // the pin, and the + and − buttons zoom.
  letPageScroll() {
    this.map.scrollWheelZoom.disable();
    if (matchMedia("(pointer: coarse)").matches) {
      this.map.dragging.disable();
      this.map.touchZoom.disable();
    }
    if (this.hasToggleTarget) this.toggleTarget.hidden = false;
  }

  // Folds the map away so the results take its place, and brings it back: the map page's
  // "Fshih hartën" (shown once there is a pin), or the header of the card on category pages.
  toggleMap() {
    const fold = !this.element.classList.contains("map-hidden");
    this.fold(fold);
    if (this.rememberValue) remember(FOLDED, fold);
    if (!fold && this.map) {
      // Leaflet measures its box; it had none while hidden.
      this.map.invalidateSize();
      this.fitView();
    }
  }

  fold(fold) {
    this.element.classList.toggle("map-hidden", fold);
    if (!this.hasToggleTarget) return;
    this.toggleTarget.setAttribute("aria-expanded", String(!fold));
    const label = fold ? this.toggleTarget.dataset.showLabel : this.toggleTarget.dataset.hideLabel;
    if (label) this.toggleTarget.textContent = label;
  }

  // While the slider moves: the distance beside it and the circle follow.
  radiusInput() {
    const km = Math.max(1, Math.round(MAX_RADIUS_KM ** (Number(this.sliderTarget.value) / 100)));
    this.radiusTarget.value = km;
    this.radiusLabelTarget.textContent = `${km} km`;
    this.sliderTarget.setAttribute("aria-valuetext", `${km} km`);
    this.circle?.setRadius(this.radiusMeters);
  }

  // Once it is let go, the map fits the circle and the results follow.
  radiusChanged() {
    this.radiusInput();
    if (!this.circle) return;
    this.fitCircle();
    this.submit();
  }

  // "Vendndodhja ime": the pin goes where the visitor is, if the browser may say.
  locate() {
    if (!this.map) return;
    if (!navigator.geolocation) {
      this.statusTarget.textContent = "Ky shfletues nuk e jep vendndodhjen.";
      return;
    }
    this.statusTarget.textContent = "Po kërkoj vendndodhjen…";
    navigator.geolocation.getCurrentPosition(
      ({ coords }) => {
        this.statusTarget.textContent = "";
        this.place(this.L.latLng(coords.latitude, coords.longitude));
      },
      () => { this.statusTarget.textContent = "Vendndodhja nuk u gjet. Prek hartën për të vendosur pikën."; },
      { enableHighAccuracy: false, timeout: 10000, maximumAge: 300000 },
    );
  }

  submit() {
    // A whole new page would open at its top; it keeps this one's place instead, the map in view.
    if (!this.hasResultsTarget) {
      const top = window.scrollY;
      document.addEventListener("turbo:load", () => window.scrollTo(0, top), { once: true });
    }
    this.formTarget.requestSubmit();
  }

  // The results on the map, read from the list the server sent (every result, not only the cards
  // loaded so far). Without a pin, the map first shows where they are.
  plot(event) {
    // The list's next pages load as frames inside the results; their points are already there.
    if (!this.map || (event && event.target !== this.resultsTarget)) return;
    this.points = [...this.pointsTarget.children].map((point) => ({
      latlng: this.L.latLng(Number(point.dataset.lat), Number(point.dataset.lng)),
      title: point.textContent,
      price: point.dataset.price,
      url: point.dataset.url,
    }));
    if (!this.pin && !this.fitted && this.points.length) {
      this.map.fitBounds(this.L.latLngBounds(this.points.map((point) => point.latlng)), { padding: [24, 24], maxZoom: 13 });
      this.fitted = true;
    }
    this.drawPoints();
  }

  // Close up (streets readable), a dot for each result, and a tap on one opens it. Further out,
  // results near each other are one numbered bubble and a lone one a small dot; neither takes
  // taps, so a tap there drops the pin, as it should while choosing a place. Redrawn on every zoom.
  drawPoints() {
    if (!this.map || !this.points) return;
    this.dots.clearLayers();
    const zoom = this.map.getZoom();
    if (zoom >= CLOSE_ZOOM) {
      for (const point of this.points) {
        const popup = document.createElement("a");
        popup.className = "map-popup";
        popup.href = point.url;
        popup.append(
          Object.assign(document.createElement("strong"), { textContent: point.title }),
          Object.assign(document.createElement("span"), { textContent: point.price }),
        );
        // A tap on a dot opens its listing's popup without moving the pin.
        this.L.circleMarker(point.latlng, {
          radius: 5, className: "map-dot", weight: 2, fillOpacity: 1, bubblingMouseEvents: false,
        }).bindPopup(popup).addTo(this.dots);
      }
      return;
    }
    // Results in the same square of the screen, CELL pixels a side, make one bubble.
    const cells = new Map();
    for (const point of this.points) {
      const { x, y } = this.map.project(point.latlng, zoom);
      const key = `${Math.floor(x / CELL)}:${Math.floor(y / CELL)}`;
      cells.set(key, [...(cells.get(key) ?? []), point]);
    }
    for (const group of cells.values()) {
      const lat = group.reduce((sum, point) => sum + point.latlng.lat, 0) / group.length;
      const lng = group.reduce((sum, point) => sum + point.latlng.lng, 0) / group.length;
      if (group.length === 1) {
        this.L.circleMarker([lat, lng], {
          radius: 3.5, className: "map-dot", weight: 1.5, fillOpacity: 1, interactive: false,
        }).addTo(this.dots);
        continue;
      }
      const size = group.length < 10 ? 22 : group.length < 100 ? 26 : 30;
      const icon = this.L.divIcon({ className: "map-cluster", html: String(group.length), iconSize: [size, size] });
      this.L.marker([lat, lng], { icon, interactive: false, keyboard: false }).addTo(this.dots);
    }
  }

  // The circle if there is a pin, otherwise the results, otherwise the whole country.
  fitView() {
    if (this.pin) {
      this.fitCircle();
    } else if (this.points?.length) {
      this.map.fitBounds(this.L.latLngBounds(this.points.map((point) => point.latlng)), { padding: [24, 24], maxZoom: 13 });
    } else {
      this.map.setView(ALBANIA.center, ALBANIA.zoom);
    }
  }

  fitCircle() {
    this.map.fitBounds(this.circleBounds, { padding: [16, 16] });
  }

  // Worked out from the pin rather than asked of the circle, which can't tell until the map
  // has had its first view.
  get circleBounds() {
    return this.pin.getLatLng().toBounds(this.radiusMeters * 2);
  }

  get radiusMeters() {
    return Number(this.radiusTarget.value) * 1000;
  }
}

// sessionStorage can be missing or refuse (private windows, blocked site data); the card then
// just starts open.
function remembered(key) {
  try { return sessionStorage.getItem(key) === "1"; } catch { return false; }
}

function remember(key, on) {
  try { on ? sessionStorage.setItem(key, "1") : sessionStorage.removeItem(key); } catch { /* not kept */ }
}
