import { Controller } from "@hotwired/stimulus";
import { loadLeaflet, tiles } from "controllers/leaflet.js";

// Where a property or a car is, more exactly than its town's point, on the listing form: the field
// shows while the category chosen is one whose pages show a map (data-mapped on its option);
// "Vendose në hartë" opens a map under it, on the point given before, or else the town chosen
// (towns: each town's point by its slug), with the pin standing in the middle as on the map page
// (.map-center, map.css), and the map moved under it. Once the seller moves it, the middle is the
// point (the hidden latitude and longitude, which the server rounds to about a hundred metres and
// checks are near the town); "Hiqe pikën" takes it off, and the listing stands at its town's
// point. A town chosen while the map is open, and no point given yet, moves the map to it.

const TIRANA = [41.3275, 19.8187];
const PIN = '<svg viewBox="0 0 24 24" width="34" height="34" aria-hidden="true"><path fill="currentColor" fill-rule="evenodd" d="M12 2a7 7 0 0 0-7 7c0 5.25 7 13 7 13s7-7.75 7-13a7 7 0 0 0-7-7Zm0 9.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z"/></svg>';

export default class extends Controller {
  static targets = ["latitude", "longitude", "canvas", "open", "openLabel", "clear", "status"];
  static values = { towns: Object };

  connect() {
    this.form = this.element.closest("form");
    this.changed = (event) => {
      if (event.target.name === "category") this.follow();
      if (event.target.name === "city" && this.map && !this.latitudeTarget.value) this.map.setView(this.town(), 13);
    };
    this.form.addEventListener("change", this.changed);
    this.follow();
  }

  disconnect() {
    this.form.removeEventListener("change", this.changed);
    this.map?.remove();
    this.map = null;
  }

  // Shown for a property or a car; for another category, hidden, and its point let go.
  follow() {
    const mapped = this.form.elements.category?.selectedOptions?.[0]?.dataset.mapped !== undefined;
    this.element.hidden = !mapped;
    if (!mapped && this.latitudeTarget.value) this.clear();
  }

  town() {
    return this.townsValue[this.form.elements.city?.value] ?? TIRANA;
  }

  point() {
    return this.latitudeTarget.value ? [Number(this.latitudeTarget.value), Number(this.longitudeTarget.value)] : null;
  }

  async open() {
    if (this.map) return;
    this.canvasTarget.hidden = false;
    this.openTarget.hidden = true;
    let L;
    try {
      L = await loadLeaflet();
    } catch {
      this.statusTarget.textContent = "Harta nuk u ngarkua. Provo përsëri.";
      this.canvasTarget.hidden = true;
      this.openTarget.hidden = false;
      return;
    }
    if (!this.element.isConnected) return;
    this.canvasTarget.replaceChildren();
    // A pinch, a double tap, the wheel and the + and − zoom around the middle, so the pin stays on
    // its place.
    this.map = L.map(this.canvasTarget, { scrollWheelZoom: "center", touchZoom: "center", doubleClickZoom: "center" });
    tiles(L).addTo(this.map);
    const point = this.point();
    this.map.setView(point ?? this.town(), point ? 16 : 13);
    const pin = Object.assign(document.createElement("div"), { className: "map-center", innerHTML: PIN });
    this.map.getContainer().append(pin);
    this.map.on("movestart", () => pin.classList.add("is-moving"));
    // Moved by the seller (a drag or a zoom; not the map's own first view, nor a town's), the
    // middle becomes the point.
    this.map.on("dragstart zoomstart", () => { this.moved = true; });
    this.map.on("moveend", () => {
      pin.classList.remove("is-moving");
      if (!this.moved) return;
      this.moved = false;
      this.place();
    });
    this.statusTarget.textContent = point
      ? "Njoftimi del te pika që vendose, afërsisht. Lëviz hartën për ta ndryshuar."
      : "Lëviz hartën derisa kunja të jetë te vendi. Ruhet afërsisht, rreth 100 m.";
    this.clearTarget.hidden = false;
  }

  place() {
    const { lat, lng } = this.map.getCenter();
    this.latitudeTarget.value = lat.toFixed(5);
    this.longitudeTarget.value = lng.toFixed(5);
    this.statusTarget.textContent = "Njoftimi del këtu, afërsisht, rreth 100 m.";
  }

  clear() {
    this.latitudeTarget.value = "";
    this.longitudeTarget.value = "";
    this.map?.remove();
    this.map = null;
    this.canvasTarget.replaceChildren();
    this.canvasTarget.hidden = true;
    this.openTarget.hidden = false;
    this.openLabelTarget.textContent = "Vendose në hartë";
    this.clearTarget.hidden = true;
    this.statusTarget.textContent = "Pa të, njoftimi del te qendra e qytetit.";
  }
}
