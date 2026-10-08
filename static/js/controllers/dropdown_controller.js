import { Controller } from "@hotwired/stimulus";

// A <details> menu or card (the order; on a listing, the loan calculator and what a square metre
// costs) closes again on a tap outside it or on Escape, the way menus do; opening and choosing
// need no JavaScript. The order's is one of the
// three beside each other that are open one at a time, as the filters and the map are: opened, it
// says so (announce, section:open), and another opening closes it (foldFor).
export default class extends Controller {
  connect() {
    this.closeOutside = (event) => {
      if (this.element.open && !this.element.contains(event.target)) this.element.open = false;
    };
    this.closeOnEscape = (event) => {
      if (event.key === "Escape" && this.element.open) {
        this.element.open = false;
        this.element.querySelector("summary")?.focus();
      }
    };
    // While the page refreshes in place (refresh.js), an open menu stays open.
    this.keepOpen = (event) => {
      if (event.target === this.element && event.detail.attributeName === "open") event.preventDefault();
    };
    document.addEventListener("click", this.closeOutside);
    document.addEventListener("keydown", this.closeOnEscape);
    this.element.addEventListener("turbo:before-morph-attribute", this.keepOpen);
  }

  announce() {
    if (this.element.open) this.dispatch("open", { prefix: "section" });
  }

  // Another card opened.
  foldFor(event) {
    if (event.target !== this.element) this.close();
  }

  // Also before Turbo caches the page for the back button, which shows it closed.
  close() {
    this.element.open = false;
  }

  disconnect() {
    document.removeEventListener("click", this.closeOutside);
    document.removeEventListener("keydown", this.closeOnEscape);
    this.element.removeEventListener("turbo:before-morph-attribute", this.keepOpen);
  }
}
