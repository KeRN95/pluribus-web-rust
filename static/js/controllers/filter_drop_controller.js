import { Controller } from "@hotwired/stimulus";
import { refreshFor } from "controllers/refresh.js";

// One of the row's filter dropdowns (filter_drop.html): a filter in a card under its pill, in a form
// of its own that carries the rest of the page along hidden. A single choice (an option of its list,
// a kind of listing, a town, a price's quick choice) applies at once, folding the card; boxes, a
// range or a typed price apply as it folds (a tap outside, Escape, another of the row's opening, its
// button), if they changed: the page refreshes in place, as for the sheet's filters (refresh.js),
// and the filter, on now, is a chip in the row in place of the pill. Opening and folding are
// dropdown_controller.js's.
export default class extends Controller {
  static targets = ["form"];

  disconnect() {
    this.start = undefined;
  }

  // Opened, the form as it is, to tell as it folds whether anything changed; folded, applied if so.
  toggled() {
    if (this.element.open) this.start = this.state();
    else if (this.start !== undefined && this.state() !== this.start) refreshFor(this.formTarget);
  }

  // A choice that is the whole of it.
  picked() {
    this.element.open = false;
  }

  // A price's quick choice, once price_range_controller.js has set the prices.
  chose(event) {
    if (event.target.closest(".price-choice")) this.picked();
  }

  // Its button, or Enter in a typed field.
  submit(event) {
    event.preventDefault();
    this.element.open = false;
  }

  state() {
    return new URLSearchParams(new FormData(this.formTarget)).toString();
  }
}
