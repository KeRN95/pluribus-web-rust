import { Controller } from "@hotwired/stimulus";

// The button beside the view menu on a category's page: it shows "Më të klikuarat" or folds it
// away, at once, pressed while it shows. A cookie remembers the choice, so the categories' pages
// after come the same way (browse.rs, MOST_VIEWED_COOKIE); folded, the row stays in the page,
// hidden, and its photos wait until it shows again.
export default class extends Controller {
  toggle() {
    const row = document.getElementById(this.element.getAttribute("aria-controls"));
    if (!row) return;
    const shown = row.hidden;
    row.hidden = !shown;
    this.element.setAttribute("aria-pressed", String(shown));
    document.cookie = shown
      ? "most-viewed=; path=/; max-age=0; samesite=lax"
      : "most-viewed=hidden; path=/; max-age=31536000; samesite=lax";
  }
}
