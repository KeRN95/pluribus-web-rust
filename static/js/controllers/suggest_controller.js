import { Controller } from "@hotwired/stimulus";

// Suggestions under a search box as its words are typed (web/suggestions.rs sends them as rows of
// links): the words finished as listings use them, the categories they are in, makes and models,
// towns. The box keeps the focus: the arrow keys mark a row, Enter follows it (or, with none
// marked, searches the words as typed), Escape closes the list; a click or a tap follows a row as
// any link. Without this script the box is a plain search.

let count = 0;
// After the last key, so that words typed quickly aren't asked for letter by letter.
const WAIT = 120;
// Suggestions already fetched, by the words they are for: deleting a letter shows them at once.
const KEPT = 40;

export default class extends Controller {
  static targets = ["input", "list"];

  connect() {
    this.kept = new Map();
    this.marked = -1;
    this.listTarget.id ||= `suggestions-${++count}`;
    const input = this.inputTarget;
    input.setAttribute("role", "combobox");
    input.setAttribute("aria-autocomplete", "list");
    input.setAttribute("aria-controls", this.listTarget.id);
    input.setAttribute("aria-expanded", "false");
    input.autocomplete = "off";
    this.close();
    this.outside = (event) => {
      if (!this.element.contains(event.target)) this.close();
    };
    document.addEventListener("click", this.outside);
  }

  disconnect() {
    document.removeEventListener("click", this.outside);
    clearTimeout(this.timer);
    this.request?.abort();
  }

  // The words changed: their suggestions, once typing pauses.
  typed() {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.fetch(), WAIT);
  }

  // Back in the box: the suggestions for its words again.
  focused() {
    if (this.listTarget.hidden) this.fetch();
  }

  async fetch() {
    const words = this.inputTarget.value;
    if (words.trim().length < 2) return this.close();
    if (this.kept.has(words)) return this.show(this.kept.get(words));
    this.request?.abort();
    this.request = new AbortController();
    let rows;
    try {
      const response = await fetch(`/sugjerime?q=${encodeURIComponent(words)}`, { signal: this.request.signal });
      if (!response.ok) return;
      rows = await response.text();
    } catch {
      // Aborted for newer words, or offline: the box still searches.
      return;
    }
    if (this.kept.size >= KEPT) this.kept.delete(this.kept.keys().next().value);
    this.kept.set(words, rows);
    // The words may have changed while these came.
    if (this.inputTarget.value === words) this.show(rows);
  }

  show(rows) {
    this.listTarget.innerHTML = rows;
    this.options().forEach((option, index) => {
      option.id = `${this.listTarget.id}-${index}`;
      option.setAttribute("aria-selected", "false");
    });
    this.mark(-1);
    const open = this.options().length > 0 && document.activeElement === this.inputTarget;
    this.listTarget.hidden = !open;
    this.inputTarget.setAttribute("aria-expanded", String(open));
  }

  close() {
    this.mark(-1);
    this.listTarget.hidden = true;
    this.inputTarget.setAttribute("aria-expanded", "false");
  }

  options() {
    return [...this.listTarget.querySelectorAll('[role="option"]')];
  }

  // Marks a row, or none (-1): the box says which, for screen readers.
  mark(index) {
    const options = this.options();
    this.marked = index;
    options.forEach((option, at) => option.setAttribute("aria-selected", String(at === index)));
    if (options[index]) {
      this.inputTarget.setAttribute("aria-activedescendant", options[index].id);
      options[index].scrollIntoView({ block: "nearest" });
    } else {
      this.inputTarget.removeAttribute("aria-activedescendant");
    }
  }

  key(event) {
    const options = this.options();
    if (this.listTarget.hidden || !options.length) {
      if (event.key === "ArrowDown") this.fetch();
      return;
    }
    switch (event.key) {
      // Down from the last row, and up from the first, go back to the words.
      case "ArrowDown": this.mark(this.marked + 1 < options.length ? this.marked + 1 : -1); break;
      case "ArrowUp": this.mark(this.marked < 0 ? options.length - 1 : this.marked - 1); break;
      case "Enter":
        if (this.marked < 0) return;
        options[this.marked].click();
        break;
      case "Escape":
        // The list closes, not what holds it too (the filters close on Escape as well).
        event.stopPropagation();
        this.close();
        break;
      default: return;
    }
    event.preventDefault();
  }

  // Focus gone elsewhere than a row: the list closes.
  left(event) {
    if (!this.element.contains(event.relatedTarget)) this.close();
  }
}
