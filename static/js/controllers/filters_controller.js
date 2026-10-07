import { Controller } from "@hotwired/stimulus";

// The filter panel: a card the round "Filtrat" button opens over the results and closes, so
// several options can be picked and then applied together with its own button. Of what the browse
// row opens (the search, the filters, the map, the order) only one is open at a time: opening it
// says so (section:open), and another's opening folds it (foldFor). Being the browse page's own,
// it also folds whatever is open on Escape or a tap on the dim under it (foldAll), and fits what
// opens into the screen (fit).
export default class extends Controller {
  static targets = ["panel", "form", "toggle", "tools", "scrim"];

  toggle() {
    if (this.panelTarget.classList.contains("is-open")) this.reset();
    else this.open();
  }

  open() {
    this.panelTarget.classList.add("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "true");
    this.dispatch("open", { target: this.toggleTarget, prefix: "section" });
  }

  // Closes it without moving focus, e.g. before Turbo caches the page for the back button.
  reset() {
    this.panelTarget.classList.remove("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "false");
  }

  // Another card opened.
  foldFor(event) {
    if (event.target !== this.toggleTarget) this.reset();
  }

  // Escape, or a tap on the dim: whatever the row opened folds, as when another opens
  // (section:open, here from the dim, which opens nothing), and the focus goes back to its button
  // if it was in it. The order's menu folds on its own (dropdown_controller.js).
  foldAll() {
    const toggle = this.toolsTarget.querySelector('.round-button[aria-expanded="true"]');
    if (!toggle) return;
    const inside = document.getElementById(toggle.getAttribute("aria-controls"))?.contains(document.activeElement);
    this.dispatch("open", { target: this.scrimTarget, prefix: "section" });
    if (inside) toggle.focus();
  }

  // What opens floats under the row and takes at most what is left of the screen under it
  // (--top, app.css), so that its foot shows; it scrolls inside if it needs more.
  fit() {
    this.toolsTarget.style.setProperty("--top", `${Math.round(this.toolsTarget.getBoundingClientRect().bottom)}px`);
  }

  submit() {
    this.formTarget.requestSubmit();
  }
}
