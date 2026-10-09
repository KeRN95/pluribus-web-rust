import { Controller } from "@hotwired/stimulus";
import { loadLeaflet, tiles } from "controllers/leaflet.js";

// Where a property or a car is, under its seller (listing.html): a map round its point, which is
// its place's (its town's, for a zone without one of its own), so a circle RADIUS metres round it,
// in the location colour (listing.css), rather than a pin, the map fitted round the circle. It
// loads, Leaflet and the tiles, only as it comes near the screen. A finger scrolls the page over
// it, as anywhere else, and two pinch it, about its middle; the + and − zoom it, and a mouse drags
// it; the wheel scrolls the page.

const RADIUS = 600;
// How near the screen it is when it starts loading.
const AHEAD = "400px 0px";

export default class extends Controller {
  static targets = ["canvas"];
  static values = { latitude: Number, longitude: Number };

  connect() {
    this.watcher = new IntersectionObserver((entries) => {
      if (!entries.some((entry) => entry.isIntersecting)) return;
      this.watcher.disconnect();
      this.draw();
    }, { rootMargin: AHEAD });
    this.watcher.observe(this.element);
  }

  disconnect() {
    this.watcher.disconnect();
    this.map?.remove();
    this.map = null;
  }

  async draw() {
    let L;
    try {
      L = await loadLeaflet();
    } catch {
      return; // The map's grey stays, the place's name over it.
    }
    if (!this.element.isConnected || this.map) return;
    // A page Turbo shows again from its cache still holds the old map's markup.
    this.canvasTarget.replaceChildren();
    const point = L.latLng(this.latitudeValue, this.longitudeValue);
    this.map = L.map(this.canvasTarget, {
      dragging: !L.Browser.mobile,
      touchZoom: "center",
      doubleClickZoom: "center",
      scrollWheelZoom: false,
      zoomSnap: 0.25,
      zoomControl: false,
    });
    // Round the circle: the square it fits in, which needs no view of the map to be worked out, as
    // the circle's own bounds do.
    this.map.fitBounds(point.toBounds(RADIUS * 2), { padding: [20, 20] });
    L.control.zoom({ position: "bottomright" }).addTo(this.map);
    tiles(L).addTo(this.map);
    L.circle(point, { radius: RADIUS, className: "listing-map-area", interactive: false }).addTo(this.map);
  }
}
