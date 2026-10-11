import { Controller } from "@hotwired/stimulus";
import { forgetSearches, recentSearches } from "controllers/recent_searches.js";
import { t } from "controllers/i18n.js";

// Suggestions under a search box as its words are typed (web/suggestions.rs sends them as rows of
// links): the words finished as listings use them, the categories they are in, makes and models,
// towns. The box keeps the focus: the arrow keys mark a row, Enter follows it (or, with none
// marked, searches the words as typed), Escape closes the list; a click or a tap follows a row as
// any link. Empty, the box offers the visitor's last searches instead (recent_searches.js), and a
// last row that forgets them; the list is named for which it holds. While the box has words, a
// cross empties it (the welcome page's, its clear target). Without this script the box is a plain
// search.
//
// A box with a way back (its back target, the welcome page's) opens over the whole screen of a
// phone as it gets the focus, as Google's search does (the user's ask of 2026-10-11, "when search
// is clicked make it open like google search bar"; welcome.css, .is-open), its list staying while
// it is open, the keyboard put away or not; the way back closes it, as Escape does, Android's back
// (a CloseWatcher, where the browser has one) and the page left. While its list shows the box is
// marked (has-list), from 720px up the pill and the list drawn as one card.

let count = 0;
// After the last key, so that words typed quickly aren't asked for letter by letter.
const WAIT = 120;
// Suggestions already fetched, by the words they are for: deleting a letter shows them at once.
const KEPT = 40;
// A phone's screen, as welcome.css has it.
const PHONE = matchMedia("(max-width: 719px)");

// A clock, Phosphor's as the site's icons are (icons.rs, CLOCK): before a last search's words.
const CLOCK = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" aria-hidden="true"><g transform="scale(.09375)" stroke="none"><path d="M224,128a96,96,0,1,1-96-96A96,96,0,0,1,224,128Z"/><path fill="currentColor" d="M128,24A104,104,0,1,0,232,128,104.11,104.11,0,0,0,128,24Zm0,192a88,88,0,1,1,88-88A88.1,88.1,0,0,1,128,216Zm64-88a8,8,0,0,1-8,8H128a8,8,0,0,1-8-8V72a8,8,0,0,1,16,0v48h48A8,8,0,0,1,192,128Z"/></g></svg>';
function clock() {
  const holder = document.createElement("template");
  holder.innerHTML = CLOCK;
  return holder.content.firstElementChild;
}

export default class extends Controller {
  static targets = ["input", "list", "clear", "back"];

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
    this.cleared();
    this.outside = (event) => {
      if (!this.element.contains(event.target)) this.close();
    };
    document.addEventListener("click", this.outside);
  }

  disconnect() {
    document.removeEventListener("click", this.outside);
    clearTimeout(this.timer);
    this.request?.abort();
    this.watcher?.destroy();
  }

  // The words changed: their suggestions, once typing pauses.
  typed() {
    this.cleared();
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.fetch(), WAIT);
  }

  // Back in the box: the suggestions for its words again, over the whole screen on a phone.
  focused() {
    this.open();
    if (this.listTarget.hidden) this.fetch();
  }

  get isOpen() {
    return this.element.classList.contains("is-open");
  }

  // Over the whole screen: a phone's box with its way back.
  open() {
    if (!this.hasBackTarget || !PHONE.matches || this.isOpen) return;
    this.element.classList.add("is-open");
    if (!("CloseWatcher" in window)) return;
    try {
      this.watcher = new CloseWatcher();
      this.watcher.addEventListener("close", () => this.dismiss());
    } catch {
      this.watcher = null;
    }
  }

  // Closed again, the page as it was under it: the way back, Escape, Android's back, or the page
  // about to be kept by Turbo as it is left.
  dismiss() {
    this.watcher?.destroy();
    this.watcher = null;
    this.close();
    if (!this.isOpen) return;
    this.element.classList.remove("is-open");
    this.inputTarget.blur();
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
    this.listTarget.setAttribute("aria-label", t("Sugjerime"));
    this.opened();
  }

  // The cross shows while the box has words.
  cleared() {
    if (this.hasClearTarget) this.clearTarget.hidden = !this.inputTarget.value;
  }

  // The cross: the box empty again, its last searches under it.
  clear() {
    this.inputTarget.value = "";
    this.cleared();
    this.inputTarget.focus();
    this.showRecent();
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
    forget.textContent = t("Pastro kërkimet e fundit");
    forget.addEventListener("click", () => {
      forgetSearches();
      this.close();
      this.inputTarget.focus();
    });
    this.listTarget.replaceChildren(...rows, forget);
    this.listTarget.setAttribute("aria-label", t("Kërkimet e fundit"));
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
    const open = this.options().length > 0 && (document.activeElement === this.inputTarget || this.isOpen);
    this.listTarget.hidden = !open;
    this.element.classList.toggle("has-list", open);
    this.inputTarget.setAttribute("aria-expanded", String(open));
  }

  close() {
    this.mark(-1);
    this.listTarget.hidden = true;
    this.element.classList.remove("has-list");
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
    // Escape closes a search open over the whole screen, its list and all.
    if (event.key === "Escape" && this.isOpen) {
      event.stopPropagation();
      event.preventDefault();
      this.dismiss();
      return;
    }
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

  // Focus gone elsewhere than a row: the list closes; a search open over the whole screen closes
  // only for somewhere else on the page (the keyboard put away leaves it as it is).
  left(event) {
    if (this.element.contains(event.relatedTarget)) return;
    if (!this.isOpen) {
      this.close();
      return;
    }
    if (event.relatedTarget) this.dismiss();
  }
}
