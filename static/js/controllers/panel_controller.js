import { Controller } from "@hotwired/stimulus";

// A panel that a round button opens and folds again: the search box, the browse row's under the row
// (one of the row's, which are open one at a time, section:open, as the filters and the map are),
// and the welcome page's over its categories (home.html). Opened, its box takes the focus, so that
// a phone shows its keyboard at once.
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

  // While the page refreshes in place (refresh.js, after the filters or the map changed), an open
  // box stays open.
  keep(event) {
    if (!this.panelTarget.classList.contains("is-open")) return;
    const { attributeName } = event.detail;
    if ((event.target === this.panelTarget && attributeName === "class")
      || (event.target === this.toggleTarget && attributeName === "aria-expanded")) event.preventDefault();
  }
}
