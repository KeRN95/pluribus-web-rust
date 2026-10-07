import { Controller } from "@hotwired/stimulus";

// A card folded to its header, which opens what it holds and folds it again: the order (Rendit),
// beside the filters and the map. Of the three only one is open at a time: opening one says so
// (section:open), and the others fold (foldFor), as the filters and the map do.
export default class extends Controller {
  static targets = ["toggle", "panel"];

  toggle() {
    if (this.panelTarget.classList.contains("is-open")) this.close();
    else this.open();
  }

  open() {
    this.panelTarget.classList.add("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "true");
    this.dispatch("open", { target: this.toggleTarget, prefix: "section" });
  }

  // Also before Turbo caches the page for the back button, which shows it folded.
  close() {
    this.panelTarget.classList.remove("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "false");
  }

  // Another card opened.
  foldFor(event) {
    if (event.target !== this.toggleTarget) this.close();
  }
}
