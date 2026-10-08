import { Controller } from "@hotwired/stimulus";
import { refreshFor, refreshTo } from "controllers/refresh.js";
import { locate } from "controllers/locate.js";
import { forgetTown } from "controllers/place_controller.js";

// A map of the results, as Facebook's: its pin stays in the middle and the map moves under it,
// on the point set before, the town picked, or Tirana's middle (filters.rs, map_start). Round the
// pin, a circle that stays the same size in the middle of the map whatever the zoom: how far
// around the pin to look is what it covers, so zooming in makes the distance smaller and zooming
// out bigger, from 1 to 50 km in whole kilometres, said over the map as it changes (the zoom, once
// it stops, settling on the one whose circle covers exactly that), and the zoom goes no further
// either way. On the map page (/harta) "Vendos hartën" sends the map's middle and the distance,
// which reloads only the results panel. On a category or search page the map is in a card that
// always comes folded, and loads, with its dots, only when it first opens (lazy); there the circle
// round the pin shows only once a point is set (ring), and the middle and the distance apply as soon as they change, when the map stops after a drag
// or a zoom, or "Vendndodhja ime" moves it (pointChanged), in place of a town; a town from the
// card's menu puts the page in it instead, the map moving onto it (pickTown): the page refreshes
// in place (refresh.js), which keeps the category and the filters, the card open as it is and the
// map as Leaflet drew it (keep), its dots following the new results (reloadPoints). Each result
// gets a dot on the map: those within the circle once it is set. Leaflet draws the map with
// OpenStreetMap's tiles.

const LEAFLET = "/static/vendor/leaflet-1.9.4";
const TIRANA = [41.3275, 19.8187];
// From this zoom on (streets readable), or from the closest the circle allows if that is further
// out, results show one by one and open on a tap; further out they are grouped by squares of the
// screen, CELL pixels a side, and taps go to the map.
const CLOSE_ZOOM = 15;
const CELL = 40;
// The circle's radius as a share of the map's smaller side, and the distances it may cover, in
// kilometres: as in filters.rs (MAX_RADIUS_KM).
const RING = 0.38;
const MIN_KM = 1;
const MAX_KM = 50;
// How long the zoom stays still before the distance follows it, in milliseconds.
const SETTLE_MS = 400;
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
    "canvas", "form", "latitude", "longitude", "radius", "radiusLabel", "status", "results", "points", "toggle",
    "frame", "town",
  ];
  // lazy: the map loads when its card is first opened (category and search pages). ring: a point
  // is set, and its circle shows.
  static values = { latitude: Number, longitude: Number, lazy: Boolean, ring: { type: Boolean, default: true } };

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
    // its place; the mouse wheel scrolls the page, which goes on below the map. Any zoom, not only
    // whole ones, as the distance follows it: a tap on + or − is half a zoom, the distance about
    // 1.4 times smaller or bigger, and a pinch stops at the closest and the furthest the circle
    // may cover (fit). A flick glides on a little, not across the Balkans (Leaflet's own top speed
    // has no end).
    this.map = L.map(this.canvasTarget, {
      scrollWheelZoom: false, touchZoom: "center", doubleClickZoom: "center", inertiaMaxSpeed: 1500,
      zoomSnap: 0, zoomDelta: 0.5, bounceAtZoomLimits: false, zoomControl: !this.lazyValue,
    });
    // In a card the + and − sit at the bottom right, clear of the pills over the map's top (map.css).
    if (this.lazyValue) L.control.zoom({ position: "bottomright" }).addTo(this.map);
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
    // The circle round the middle, the same size whatever the zoom: in a pane of its own, over the
    // tiles and under the dots (Leaflet's overlays are at 400), kept in the middle as the map moves
    // (placeRing). Not Leaflet's own circle, which is a size on the ground rather than on the
    // screen, and stays in the middle only by being moved on the map at every step.
    this.map.createPane("ring").style.zIndex = 350;
    this.ring = Object.assign(document.createElement("div"), { className: "map-area" });
    this.map.getPane("ring").append(this.ring);
    this.ringValueChanged();
    this.map.on("move zoom viewreset", () => this.placeRing());
    // The distance follows the zoom: as a pinch goes, and as the + and − start theirs; once it
    // stops, the map settles on it (zoomed). A map whose box changed (a phone turned) keeps it.
    this.map.on("zoom", () => this.showDistance(this.map.getZoom()));
    this.map.on("zoomanim", ({ zoom }) => this.showDistance(zoom));
    this.map.on("zoomend", () => this.zoomed());
    this.map.on("resize", () => this.fit({ animate: false }));
    // The pin isn't on the map but over it, in the middle (map.css): the map moves, the circle
    // stays round it, and the pin lifts until the map stops.
    this.pin = Object.assign(document.createElement("div"), { className: "map-center", innerHTML: PIN });
    this.map.getContainer().append(this.pin);
    this.map.on("movestart", () => this.pin.classList.add("is-moving"));
    this.map.on("moveend", () => this.pin.classList.remove("is-moving"));
    // Moved by a finger or the mouse (a place and a town say so themselves; a zoom keeps the
    // middle, and says so on its own, zoomed): in a card, the point follows once the map stops.
    this.map.on("dragstart", () => { this.dragged = true; });
    this.map.on("moveend", () => {
      if (!this.dragged) return;
      this.dragged = false;
      this.pointChanged();
    });

    this.fit({ point, animate: false });
    if (this.hasToggleTarget) this.toggleTarget.hidden = false;
    this.plot();
  }

  disconnect() {
    clearTimeout(this.settling);
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
    this.fit({ animate: false });
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

  // While the page refreshes in place (refresh.js): the map stays as Leaflet drew it, and the line
  // under the card's pills says what this controller has it say (the new page's is empty); an open
  // card stays open, its button saying so.
  keep(event) {
    if (event.target === this.canvasTarget || event.target === this.statusTarget) event.preventDefault();
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

  // A zoom stopped: once the map has been still a moment (SETTLE_MS, so that taps on + or − in a
  // row make one change), the distance is what the circle covers, in whole kilometres, and the map
  // settles on the zoom whose circle covers exactly that, at once (an animation would swallow a tap
  // that came during it). Changed by a pinch, a tap or a button, in a card the results follow
  // (pointChanged), and on the map page "Vendos hartën" sends it; the map's own zooms (fit, and
  // the settling) keep the distance, so they change nothing.
  zoomed() {
    clearTimeout(this.settling);
    this.settling = setTimeout(() => this.settle(), SETTLE_MS);
  }

  settle() {
    if (!this.measured) return;
    const km = this.distanceAt(this.map.getZoom());
    this.showDistance(this.map.getZoom());
    if (km === this.km) return;
    this.radiusTarget.value = km;
    this.map.setZoom(this.zoomFor(km), { animate: false });
    this.pointChanged();
  }

  // The distance the circle covers at a zoom, said beside its name as it changes.
  showDistance(zoom) {
    if (this.measured) this.radiusLabelTarget.textContent = `${this.distanceAt(zoom)} km`;
  }

  // "Vendndodhja ime": the map moves to where the visitor is, if the browser may say, for
  // "Vendos hartën" to send, or in a card to apply by itself.
  async locate() {
    if (!this.map) return;
    this.statusTarget.textContent = "Po kërkoj vendndodhjen…";
    try {
      const { latitude, longitude } = await locate();
      this.statusTarget.textContent = "";
      const point = this.L.latLng(latitude, longitude);
      this.fit({ point });
      this.pointChanged(point);
    } catch (error) {
      this.statusTarget.textContent = error.message;
    }
  }

  // The map page's town: the map moves onto its point, for "Vendos hartën" to set, as with
  // "Vendndodhja ime".
  goToTown() {
    const option = this.townTarget.selectedOptions[0];
    if (!this.map || !option?.dataset.lat) return;
    const point = this.L.latLng(Number(option.dataset.lat), Number(option.dataset.lng));
    this.fit({ point });
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

  // A town from the card's menu, or all of Albania: the page refreshes in place in it
  // (refresh.js), the card open as it is, the town in place of the map's point, or no place at all,
  // which forgets the town kept for the next pages (place_controller.js). The map moves onto the
  // town, its circle gone until a point is set again by moving it (ring). The point's own entry,
  // there while one is set, leads nowhere.
  pickTown() {
    const option = this.townTarget.selectedOptions[0];
    const url = option?.dataset.url;
    if (!url) return;
    if (!option.value) forgetTown();
    this.ringValue = false;
    if (this.map && option.dataset.lat) {
      this.fit({ point: this.L.latLng(Number(option.dataset.lat), Number(option.dataset.lng)) });
    }
    refreshTo(url);
  }

  // A point set, or none (a town picked, a page without one): the circle shows or goes.
  ringValueChanged() {
    if (this.ring) this.ring.hidden = !this.ringValue;
  }

  // In a card (lazy), the map's middle and the distance apply as soon as they change, and the
  // circle shows. A place gives its own point, the map still on its way there.
  pointChanged(point) {
    if (!this.lazyValue) return;
    this.ringValue = true;
    this.apply(point);
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

  // Close up (streets readable, or as close as the circle goes), a dot for each result, and a tap
  // on one opens it. Further out, results near each other are one numbered bubble and a lone one
  // a small dot; neither takes taps, so they don't get in the way of moving the map. Redrawn on
  // every zoom.
  drawPoints() {
    if (!this.map || !this.points) return;
    this.dots.clearLayers();
    const zoom = this.map.getZoom();
    if (zoom >= Math.min(CLOSE_ZOOM, this.map.getMaxZoom() - 0.01)) {
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

  // The map on a point, the middle unless another is given, at the zoom whose circle covers the
  // distance set, between those of 1 km and 50 km, the closest and the furthest it may go (set
  // as options, not with setMinZoom and setMaxZoom, which would zoom to them on the way). A map
  // folded away has no size to measure the circle by: it fits again when it opens (toggleMap).
  fit({ point = this.map.getCenter(), animate = true } = {}) {
    if (!this.measured) {
      if (!this.placed) this.map.setView(point, 11, { animate: false });
      this.placed = true;
      return;
    }
    this.placed = true;
    this.map.options.minZoom = this.zoomFor(MAX_KM, point.lat);
    this.map.options.maxZoom = this.zoomFor(MIN_KM, point.lat);
    this.map.fire("zoomlevelschange");
    this.map.setView(point, this.zoomFor(this.km, point.lat), { animate });
  }

  // The circle round the middle of the map, the same size whatever the zoom: RING of the map's
  // smaller side, kept in the middle as the map moves.
  placeRing() {
    if (!this.ring || !this.measured) return;
    const middle = this.map.containerPointToLayerPoint(this.map.getSize().divideBy(2));
    const radius = this.ringPixels;
    Object.assign(this.ring.style, {
      left: `${middle.x - radius}px`,
      top: `${middle.y - radius}px`,
      width: `${2 * radius}px`,
      height: `${2 * radius}px`,
    });
  }

  // The zoom at which the circle covers `km` round a latitude (the map's middle's unless another
  // is given): its radius in pixels times the metres a pixel stands for there.
  zoomFor(km, lat = this.map.getCenter().lat) {
    return Math.log2((this.ringPixels * EQUATOR * Math.cos((lat * Math.PI) / 180)) / (256 * km * 1000));
  }

  // The whole kilometres the circle covers at a zoom, 1 to 50.
  distanceAt(zoom) {
    const metres = (this.ringPixels * EQUATOR * Math.cos((this.map.getCenter().lat * Math.PI) / 180)) / (256 * 2 ** zoom);
    return Math.min(MAX_KM, Math.max(MIN_KM, Math.round(metres / 1000)));
  }

  get ringPixels() {
    const size = this.map.getSize();
    return RING * Math.min(size.x, size.y);
  }

  // The map has a box to measure (not a folded card's).
  get measured() {
    return Boolean(this.map?.getSize().x);
  }

  get km() {
    return Number(this.radiusTarget.value);
  }
}
