import { Controller } from "@hotwired/stimulus";

// The filter panel: a card whose header, "Filtrat", opens and closes it, so several options can
// be picked and then applied together with its own button. Of the cards beside it (the map, the
// order) only one is open at a time: opening it says so (section:open), and another's opening
// folds it (foldFor).
export default class extends Controller {
  static targets = ["panel", "form", "toggle"];

  toggle() {
    if (this.panelTarget.classList.contains("is-open")) this.reset();
    else this.open();
  }

  open() {
    this.panelTarget.classList.add("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "true");
    this.dispatch("open", { target: this.toggleTarget, prefix: "section" });
  }

  // Escape closes it too; the focus goes back to the button if it was in the section.
  close() {
    if (!this.panelTarget.classList.contains("is-open")) return;
    const inside = this.panelTarget.contains(document.activeElement);
    this.reset();
    if (inside) this.toggleTarget.focus();
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

  submit() {
    this.formTarget.requestSubmit();
  }
}
