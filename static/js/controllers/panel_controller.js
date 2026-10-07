import { Controller } from "@hotwired/stimulus";

// A panel that a round button in the browse row opens under it and folds again: the search box.
// It is one of the row's, which are open one at a time (section:open), as the filters and the map
// are. Opened, its box takes the focus, so that a phone shows its keyboard at once.
export default class extends Controller {
  static targets = ["toggle", "panel", "input"];

  toggle() {
    if (this.panelTarget.classList.contains("is-open")) this.close();
    else this.open();
  }

  open() {
    this.panelTarget.classList.add("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "true");
    this.dispatch("open", { target: this.toggleTarget, prefix: "section" });
    if (this.hasInputTarget) this.inputTarget.focus();
  }

  // Also before Turbo caches the page for the back button, which shows it folded.
  close() {
    this.panelTarget.classList.remove("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "false");
  }

  // Another of the row's opened.
  foldFor(event) {
    if (event.target !== this.toggleTarget) this.close();
  }
}
