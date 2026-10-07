import { Controller } from "@hotwired/stimulus";
import { refreshFor } from "controllers/refresh.js";

// The filter panel: a card the round "Filtrat" button opens over the results and closes. Its
// options apply by themselves, as soon as one changes (a box ticked, a choice picked, a typed
// field left or entered, a slider let go) and as the card closes, with whatever was typed but not
// left: the page refreshes in place for them (apply, refresh.js), the card left open as the
// visitor has it (keep), and the results, their count and the chips follow under it. Of what the
// browse row opens (the search, the filters, the map, the order) only one is open at a time:
// opening it says so (section:open), and another's opening folds it (foldFor). Being the browse
// page's own, it also folds whatever is open on Escape or a tap on the dim under it (foldAll), and
// fits what opens into the screen (fit).
export default class extends Controller {
  static targets = ["panel", "form", "toggle", "tools", "scrim"];

  disconnect() {
    clearTimeout(this.timer);
  }

  toggle() {
    if (this.isOpen) this.fold();
    else this.open();
  }

  open() {
    this.panelTarget.classList.add("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "true");
    this.dispatch("open", { target: this.toggleTarget, prefix: "section" });
  }

  // Folds it, applying what was typed but not yet left.
  fold() {
    this.apply();
    this.reset();
  }

  // Folds it as it is, without moving the focus or applying, e.g. before Turbo caches the page for
  // the back button.
  reset() {
    this.panelTarget.classList.remove("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "false");
  }

  // Another card opened.
  foldFor(event) {
    if (event.target !== this.toggleTarget && this.isOpen) this.fold();
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
  // (--top, browse.css), so that its foot shows; it scrolls inside if it needs more.
  fit() {
    this.toolsTarget.style.setProperty("--top", `${Math.round(this.toolsTarget.getBoundingClientRect().bottom)}px`);
  }

  // A change in the form, a box ticked or a field left: the results follow at once, once the
  // form's other listeners have had it too (narrow_controller.js drops a model that no longer fits
  // its make).
  changed() {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.apply());
  }

  // Enter in a typed field (a year, a distance) applies it, rather than sending the form.
  enter(event) {
    if (!event.target.matches('input[type="text"]')) return;
    event.preventDefault();
    this.apply();
  }

  apply() {
    clearTimeout(this.timer);
    refreshFor(this.formTarget);
  }

  // While the page morphs into the refreshed one: the open card stays as the visitor has it, its
  // fields and all, and its button says it is open; the row keeps its measure (--top); and a field
  // in use elsewhere, the map's distance as it is dragged, keeps its value.
  keep(event) {
    if (event.target === this.panelTarget && this.isOpen) event.preventDefault();
  }

  keepAttribute(event) {
    const { attributeName } = event.detail;
    if ((event.target === this.toggleTarget && attributeName === "aria-expanded" && this.isOpen)
      || (event.target === this.toolsTarget && attributeName === "style")
      || (event.target === document.activeElement && (attributeName === "value" || attributeName === "checked"))) {
      event.preventDefault();
    }
  }

  // An order picked in Rendit's menu sends the form, and the page reloads in it.
  submit() {
    this.formTarget.requestSubmit();
  }

  get isOpen() {
    return this.panelTarget.classList.contains("is-open");
  }
}
