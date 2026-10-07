import { Controller } from "@hotwired/stimulus";
import { refreshFor } from "controllers/refresh.js";

// A map of the results, as Facebook's: its pin stays in the middle and the map moves under it,
// on the point set before or Tirana's middle (filters.rs). The slider sets how far around it to
// look. On the map page (/harta) "Vendos hartën" sends the map's middle and the distance, which
// reloads only the results panel. On a category or search page the map is in a card that always
// comes folded, and loads, with its dots, only when it first opens (lazy); there the middle and
// the distance apply as soon as they change, when the map stops after a drag, the distance is let
// go, or a place or a town is picked (pointChanged): the page refreshes in place (refresh.js),
// which keeps the category and the filters, the card open as it is and the map as Leaflet drew it
// (keep), its dots following the new results (reloadPoints). Each result gets a dot on the map.
// Leaflet draws the map with OpenStreetMap's tiles.

const LEAFLET = "/static/vendor/leaflet-1.9.4";
const TIRANA = [41.3275, 19.8187];
// From this zoom on (streets readable) results show one by one and open on a tap; further out
// they are grouped by squares of the screen, CELL pixels a side, and taps go to the map.
const CLOSE_ZOOM = 15;
const CELL = 40;
// The distance slider goes from 0 to 100 and stands for 1 to MAX_RADIUS_KM km, slowly near the
// pin and faster further out: km = MAX_RADIUS_KM ^ (position / 100). As in filters.rs.
const MAX_RADIUS_KM = 50;
// The equator's length in metres, which the map (Web Mercator) draws 256 pixels long at zoom 0 and
// twice as long at each zoom after: at a latitude, a pixel is this times its cosine over 256 * 2^zoom.
const EQUATOR = 40075016.686;
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
    "frame", "town",
  ];
  // lazy: the map loads when its card is first opened (category and search pages).
  static values = { latitude: Number, longitude: Number, lazy: Boolean };

  connect() {
    if (!this.lazyValue) {
      this.start();
      return;
    }
    // Folded, as it always comes, even on a page Turbo restores from its cache open; that page
    // still holds the old map's markup too.
    this.fold(true);
    this.canvasTarget.replaceChildren();
  }

  // Draws the map, Leaflet fetched first; once, whoever asks.
  start() {
    this.starting ??= this.draw();
    return this.starting;
  }

  async draw() {
    let L;
    try {
      L = await loadLeaflet();
    } catch {
      this.statusTarget.textContent = "Harta nuk u ngarkua. Provo përsëri.";
      this.starting = null;
      return;
    }
    if (!this.element.isConnected) return;
    this.L = L;

    // A page restored from Turbo's cache still holds the old map's markup.
    this.canvasTarget.replaceChildren();
    // A pinch, a double tap and the + and − buttons zoom around the middle, so the pin stays on
    // its place; the mouse wheel scrolls the page, which goes on below the map. A flick glides
    // on a little, not across the Balkans (Leaflet's own top speed has no end).
    this.map = L.map(this.canvasTarget, {
      scrollWheelZoom: false, touchZoom: "center", doubleClickZoom: "center", inertiaMaxSpeed: 1500,
    });
    L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
      maxZoom: 19,
      attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
    }).addTo(this.map);
    this.dots = L.layerGroup().addTo(this.map);
    this.map.on("zoomend", () => this.drawPoints());

    // The point the results are for, which the map starts on. The colours of the circle and the
    // dots come from map.css, so they follow the theme.
    const point = this.hasLatitudeValue && this.hasLongitudeValue
      ? L.latLng(this.latitudeValue, this.longitudeValue)
      : L.latLng(TIRANA);
    // The circle round the middle, as far as the distance: in a pane of its own, over the tiles and
    // under the dots (Leaflet's overlays are at 400), kept in the middle as the map moves and sized
    // for its zoom (placeRing). Not Leaflet's own circle, which stays in the middle only by being
    // moved on the map at every step: dragged far, it was cut off where the map had been until the
    // map stopped, and while a pinch zoomed, Leaflet scaled it as well, so it grew or shrank twice
    // as fast.
    this.map.createPane("ring").style.zIndex = 350;
    this.ring = Object.assign(document.createElement("div"), { className: "map-area" });
    this.map.getPane("ring").append(this.ring);
    this.map.on("move zoom", () => this.placeRing());
    this.map.on("zoomanim", ({ zoom, center }) => this.placeRing({ zoom, center, animated: true }));
    // The pin isn't on the map but over it, in the middle (map.css): the map moves, the circle
    // stays round it, and the pin lifts until the map stops.
    this.pin = Object.assign(document.createElement("div"), { className: "map-center", innerHTML: PIN });
    this.map.getContainer().append(this.pin);
    this.map.on("movestart", () => this.pin.classList.add("is-moving"));
    this.map.on("moveend", () => this.pin.classList.remove("is-moving"));
    // Moved by a finger or the mouse (the slider, a place and a town say so themselves; a zoom
    // keeps the middle): in a card, the point follows once the map stops.
    this.map.on("dragstart", () => { this.dragged = true; });
    this.map.on("moveend", () => {
      if (!this.dragged) return;
      this.dragged = false;
      this.pointChanged();
    });

    this.fitCircle({ point });
    if (this.hasToggleTarget) this.toggleTarget.hidden = false;
    this.plot();
  }

  disconnect() {
    this.map?.remove();
    this.map = this.pin = this.ring = this.starting = null;
  }

  // The map page's "Vendos hartën": the results follow the map's middle and the distance.
  confirm() {
    if (this.map) this.send(this.map.getCenter());
  }

  // Folds the map away so the results take its place, and brings it back: the map page's
  // "Fshih hartën", or the header of the card on category pages, whose map loads the first time,
  // and then the results' dots.
  async toggleMap() {
    const fold = !this.element.classList.contains("map-hidden");
    this.fold(fold);
    if (fold) return;
    if (!this.map) {
      await this.start();
      if (this.hasFrameTarget && !this.frameTarget.hasAttribute("src")) {
        this.frameTarget.src = this.frameTarget.dataset.src;
      }
      return;
    }
    // Leaflet measures its box; it had none while hidden.
    this.map.invalidateSize();
    this.fitView();
  }

  fold(fold) {
    this.element.classList.toggle("map-hidden", fold);
    if (!this.hasToggleTarget) return;
    this.toggleTarget.setAttribute("aria-expanded", String(!fold));
    const label = fold ? this.toggleTarget.dataset.showLabel : this.toggleTarget.dataset.hideLabel;
    if (label) this.toggleTarget.textContent = label;
    // In its card, beside the filters and the order: only one of the three is open at a time.
    if (!fold && this.lazyValue) this.dispatch("open", { target: this.toggleTarget, prefix: "section" });
  }

  // Another card opened.
  foldFor(event) {
    if (event.target !== this.toggleTarget && !this.element.classList.contains("map-hidden")) this.fold(true);
  }

  // While the page refreshes in place (refresh.js): the map stays as Leaflet drew it, and an open
  // card stays open, its button saying so.
  keep(event) {
    if (event.target === this.canvasTarget) event.preventDefault();
  }

  keepOpen(event) {
    if (this.element.classList.contains("map-hidden")) return;
    const { attributeName } = event.detail;
    if ((event.target === this.element && attributeName === "class")
      || (event.target === this.toggleTarget && attributeName === "aria-expanded")) event.preventDefault();
  }

  // The page refreshed: the dots follow its results, if the map is there to show them.
  reloadPoints() {
    if (this.map && this.hasFrameTarget) this.frameTarget.src = this.frameTarget.dataset.src;
  }

  // While the slider moves: the distance beside it and the circle follow, and the map zooms in
  // and out with it, at once, to keep the circle whole. The map page's "Vendos hartën" sends it; in
  // a card it applies when let go (radiusChanged).
  radiusInput() {
    const km = Math.max(1, Math.round(MAX_RADIUS_KM ** (Number(this.sliderTarget.value) / 100)));
    this.radiusTarget.value = km;
    this.radiusLabelTarget.textContent = `${km} km`;
    this.sliderTarget.setAttribute("aria-valuetext", `${km} km`);
    if (!this.map) return;
    this.placeRing();
    this.fitCircle({ animate: false });
  }

  // Let go (or moved by the keyboard): the same, for the last position; in a card, the results
  // follow.
  radiusChanged() {
    this.radiusInput();
    this.pointChanged();
  }

  // "Vendndodhja ime": the map moves to where the visitor is, if the browser may say, for
  // "Vendos hartën" to send, or in a card to apply by itself.
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
        const point = this.L.latLng(coords.latitude, coords.longitude);
        this.fitCircle({ point });
        this.pointChanged(point);
      },
      () => { this.statusTarget.textContent = "Vendndodhja nuk u gjet. Lëviz hartën për ta vendosur pikën."; },
      { enableHighAccuracy: false, timeout: 10000, maximumAge: 300000 },
    );
  }

  // A town from the dropdown: the map moves onto its point, for "Vendos hartën" to set, as with
  // "Vendndodhja ime".
  goToTown() {
    const option = this.townTarget.selectedOptions[0];
    if (!this.map || !option?.dataset.lat) return;
    const point = this.L.latLng(Number(option.dataset.lat), Number(option.dataset.lng));
    this.fitCircle({ point });
    this.pointChanged(point);
  }

  // The map page's category: the results follow, for the map as it is.
  submit() {
    this.send(this.map?.getCenter());
  }

  // The form goes, with the map's middle as the point.
  send(point) {
    this.setPoint(point);
    this.formTarget.requestSubmit();
  }

  // In a card (lazy), the map's middle and the distance apply as soon as they change. A place or a
  // town gives its own point, the map still on its way there.
  pointChanged(point) {
    if (this.lazyValue) this.apply(point);
  }

  // The page refreshes in place for the point, the map's middle unless another is given, and the
  // distance (refresh.js).
  apply(point = this.map?.getCenter()) {
    this.setPoint(point);
    refreshFor(this.formTarget);
  }

  // The map's middle as the form's point; kept on the element too, so a page Turbo caches shows
  // this point when it comes back.
  setPoint(point) {
    if (!point) return;
    this.latitudeTarget.value = point.lat.toFixed(5);
    this.longitudeTarget.value = point.lng.toFixed(5);
    this.latitudeValue = point.lat;
    this.longitudeValue = point.lng;
  }

  // The results on the map, read from the list the server sent (every result, not only the cards
  // loaded so far): on the map page in the results' frame, on a category page in the card's.
  plot(event) {
    if (!this.map || !this.hasPointsTarget) return;
    // The list's next pages load as frames inside the results too; their points are already there.
    if (event && event.target !== (this.hasResultsTarget ? this.resultsTarget : this.frameTarget)) return;
    this.points = [...this.pointsTarget.children].map((point) => ({
      latlng: this.L.latLng(Number(point.dataset.lat), Number(point.dataset.lng)),
      title: point.textContent,
      price: point.dataset.price,
      url: point.dataset.url,
    }));
    this.drawPoints();
  }

  // Close up (streets readable), a dot for each result, and a tap on one opens it. Further out,
  // results near each other are one numbered bubble and a lone one a small dot; neither takes
  // taps, so they don't get in the way of moving the map. Redrawn on every zoom.
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
        // A tap on a dot opens its listing's popup.
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

  fitView() {
    this.fitCircle();
  }

  // The map on a point, the middle unless another is given, zoomed to show its circle whole with a
  // little room around it. Only the zoom is worked out from the circle: fitBounds would centre on
  // the middle of its box, which Mercator puts a little north of the point (some 170 m for 50 km),
  // and the pin would creep north as the slider moves. A map folded away has no size to fit; it
  // fits again when it opens (toggleMap).
  fitCircle({ point = this.map.getCenter(), animate = true } = {}) {
    const zoom = this.map.getSize().x
      ? this.map.getBoundsZoom(point.toBounds(this.radiusMeters * 2), false, this.L.point(32, 32))
      : 11;
    this.map.setView(point, zoom, { animate });
  }

  // The circle round the middle of the map, as wide as the distance at the middle's latitude and
  // the zoom: the map's, a pinch's in-between one too, or, as a zoom animates, the one it goes to,
  // the circle growing or shrinking with the map (map.css). With its edge far past the map's
  // corners it only needs to cover the map, however much bigger it would be.
  placeRing({ zoom = this.map.getZoom(), center = this.map.getCenter(), animated = false } = {}) {
    const size = this.map.getSize();
    const middle = this.map.containerPointToLayerPoint(size.divideBy(2));
    const metresPerPixel = (EQUATOR * Math.cos((center.lat * Math.PI) / 180)) / 2 ** (zoom + 8);
    const radius = Math.min(this.radiusMeters / metresPerPixel, 2 * (size.x + size.y));
    this.ring.classList.toggle("is-animated", animated);
    Object.assign(this.ring.style, {
      left: `${middle.x - radius}px`,
      top: `${middle.y - radius}px`,
      width: `${2 * radius}px`,
      height: `${2 * radius}px`,
    });
  }

  get radiusMeters() {
    return Number(this.radiusTarget.value) * 1000;
  }
}
