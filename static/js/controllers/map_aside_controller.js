import { Controller } from "@hotwired/stimulus";

// A car's page from 960 px up (listing.html, .listing-car): the map moves into the column at the
// right, under the seller's card and the buttons, when that column leaves room enough under its
// content for a square map as wide as it, with its heading, the left column running on that far
// further down; and back under the details when it no longer does (a narrower window, a phone).
// It reads the layout as the page shows and again only as the two columns' box changes size (the
// window, the description unfolded); the map itself only loads as it comes near the screen
// (listing_map_controller.js), wherever it stands.

const WIDE = window.matchMedia("(min-width: 960px)");

export default class extends Controller {
  static targets = ["map", "deal"];

  connect() {
    this.queued = false;
    this.watcher = new ResizeObserver(() => this.queue());
    this.watcher.observe(this.element);
  }

  disconnect() {
    this.watcher.disconnect();
  }

  queue() {
    if (this.queued) return;
    this.queued = true;
    requestAnimationFrame(() => {
      this.queued = false;
      this.place();
    });
  }

  place() {
    const map = this.mapTarget;
    const deal = this.dealTarget;
    if (!WIDE.matches) return this.put(this.element);
    // The left column's foot and the right column's, the map left out of both: the map, last in
    // the left column, leaves nothing over it when it goes, so the two read the same either way.
    const foot = (parts) => Math.max(0, ...parts.filter((part) => part !== map).map((part) => part.getBoundingClientRect().bottom));
    const left = foot([...this.element.children].filter((part) => part !== deal));
    const right = foot([...deal.children]);
    // What the map takes there: its heading and the room over it, and a square as wide as the column.
    const box = map.querySelector(".listing-map");
    // In the screen's pixels, as the feet are: the map's own scale is apart from the page's
    // (its zoom, --page-scale), so its box is measured as it shows.
    const zoom = this.element.currentCSSZoom || 1;
    const shown = (part) => part.getBoundingClientRect().height;
    const needed = deal.getBoundingClientRect().width + (shown(map) - shown(box)) + 24 * zoom;
    this.put(left - right >= needed ? deal : this.element);
  }

  put(parent) {
    if (this.mapTarget.parentElement !== parent) parent.append(this.mapTarget);
  }
}
