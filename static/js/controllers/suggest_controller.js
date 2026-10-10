import { Controller } from "@hotwired/stimulus";
import { forgetSearches, recentSearches } from "controllers/recent_searches.js";

// Suggestions under a search box as its words are typed (web/suggestions.rs sends them as rows of
// links): the words finished as listings use them, the categories they are in, makes and models,
// towns. The box keeps the focus: the arrow keys mark a row, Enter follows it (or, with none
// marked, searches the words as typed), Escape closes the list; a click or a tap follows a row as
// any link. Empty, the box offers the visitor's last searches instead (recent_searches.js), and a
// last row that forgets them. Without this script the box is a plain search.

let count = 0;
// After the last key, so that words typed quickly aren't asked for letter by letter.
const WAIT = 120;
// Suggestions already fetched, by the words they are for: deleting a letter shows them at once.
const KEPT = 40;

// A clock, the line icons' way (icons.rs, CLOCK): before a last search's words.
function clock() {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  for (const [name, value] of Object.entries({ width: 16, height: 16, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": 2, "stroke-linecap": "round", "stroke-linejoin": "round", "aria-hidden": "true" })) {
    svg.setAttribute(name, value);
  }
  const face = document.createElementNS("http://www.w3.org/2000/svg", "circle");
  face.setAttribute("cx", 12);
  face.setAttribute("cy", 12);
  face.setAttribute("r", 9);
  const hands = document.createElementNS("http://www.w3.org/2000/svg", "path");
  hands.setAttribute("d", "M12 7v5l3 2");
  svg.append(face, hands);
  return svg;
}

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
    if (!words.trim()) return this.showRecent();
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
    this.opened();
  }

  // The last searches, each a row like a suggestion with a clock before its words, leading back
  // to its page; then "Pastro kërkimet e fundit". None, and the list stays closed. Made of
  // elements, not HTML: the words are the visitor's own.
  showRecent() {
    const searches = recentSearches();
    if (!searches.length) return this.close();
    const rows = searches.map((search) => {
      const row = document.createElement("a");
      row.className = "suggestion suggestion-recent";
      row.setAttribute("role", "option");
      row.tabIndex = -1;
      row.href = search.address;
      const label = document.createElement("span");
      label.className = "suggestion-label";
      label.textContent = search.words;
      row.append(clock(), label);
      return row;
    });
    const forget = document.createElement("button");
    forget.type = "button";
    forget.className = "suggestion suggestion-forget";
    forget.setAttribute("role", "option");
    forget.tabIndex = -1;
    forget.textContent = "Pastro kërkimet e fundit";
    forget.addEventListener("click", () => {
      forgetSearches();
      this.close();
      this.inputTarget.focus();
    });
    this.listTarget.replaceChildren(...rows, forget);
    this.opened();
  }

  // The list's rows in it: each its id for the box to name, none marked, open while the box has
  // the focus.
  opened() {
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
