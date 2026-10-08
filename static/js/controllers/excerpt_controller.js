import { Controller } from "@hotwired/stimulus";

// The start of a listing's description on a card in the list, three lines at most (page.css):
// "Lexo më shumë" shows only when it has more than those, and opens it all where it is. It measures
// again whenever the text's box changes, as when the view menu turns small cards into the list
// (cards_controller.js), which shows it, or the screen turns.
export default class extends Controller {
  static targets = ["text", "button"];

  connect() {
    this.observer = new ResizeObserver(() => this.measure());
    this.observer.observe(this.textTarget);
  }

  disconnect() {
    this.observer.disconnect();
  }

  measure() {
    if (this.element.classList.contains("is-open")) return;
    this.buttonTarget.hidden = this.textTarget.scrollHeight <= this.textTarget.clientHeight + 1;
  }

  expand() {
    this.element.classList.add("is-open");
    this.buttonTarget.hidden = true;
  }
}
