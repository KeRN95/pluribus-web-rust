import { Controller } from "@hotwired/stimulus";

// The magnifier beside the account, in a bar that folds its search away, a listing's (base.html;
// base.css, .search-folded), as the user asked on 2026-10-11: a tap opens the search where the other
// pages have it, its field ready for typing (on a phone, the keyboard up); another tap folds it
// again, and so does Escape in it once its suggestions are closed (suggest_controller.js closes
// those first, keeping the key to itself), the focus back on the magnifier. Turbo keeps the page
// folded for the back button. In every other bar the magnifier is hidden and does nothing.
export default class extends Controller {
  connect() {
    this.bar = this.element.closest(".site-header");
    this.search = this.bar?.querySelector(".top-search");
    if (!this.bar?.classList.contains("search-folded") || !this.search) return;
    this.escape = (event) => {
      if (event.key !== "Escape" || !this.open) return;
      this.fold();
      this.element.focus();
    };
    this.search.addEventListener("keydown", this.escape);
  }

  disconnect() {
    if (this.escape) this.search.removeEventListener("keydown", this.escape);
  }

  get open() {
    return this.bar?.classList.contains("search-open") ?? false;
  }

  // The field takes the focus at once, within the tap, or a phone's keyboard wouldn't come up. The
  // tap goes on to the page, where the search's own listener closes its suggestions at a click
  // outside it (suggest_controller.js); once it has, the field is told it has the focus again, so
  // that an empty one offers the last searches, as a tap in it does.
  toggle() {
    if (!this.escape) return;
    if (this.open) return this.fold();
    this.bar.classList.add("search-open");
    this.element.setAttribute("aria-expanded", "true");
    const field = this.search.querySelector("input[name=q]");
    field?.focus();
    setTimeout(() => {
      if (this.open && document.activeElement === field) field.dispatchEvent(new FocusEvent("focus"));
    });
  }

  fold() {
    if (!this.open) return;
    this.search.querySelector("input[name=q]")?.blur();
    this.bar.classList.remove("search-open");
    this.element.setAttribute("aria-expanded", "false");
  }
}
