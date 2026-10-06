import { Controller } from "@hotwired/stimulus";

const wide = window.matchMedia("(min-width: 960px)");

// The filter panel. On wide screens it sits beside the results and a picked option applies at
// once. On phones it opens as a full-screen sheet from the "Filtrat" button, so several options
// can be picked and then applied together with the sheet's button.
export default class extends Controller {
  static targets = ["panel", "form", "toggle"];

  open() {
    this.panelTarget.classList.add("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "true");
    document.documentElement.classList.add("has-sheet");
    this.panelTarget.focus();
  }

  close() {
    if (!this.panelTarget.classList.contains("is-open")) return;
    this.reset();
    this.toggleTarget.focus();
  }

  // Closes the sheet without moving focus, e.g. before Turbo caches the page for the back button.
  reset() {
    this.panelTarget.classList.remove("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "false");
    document.documentElement.classList.remove("has-sheet");
  }

  // Typed values (a price, a year) wait for Enter or the button, so typing isn't interrupted.
  change(event) {
    if (wide.matches && event.target.matches("select, input[type=checkbox], input[type=radio]")) {
      this.submit();
    }
  }

  submit() {
    this.formTarget.requestSubmit();
  }

  disconnect() {
    document.documentElement.classList.remove("has-sheet");
  }
}
