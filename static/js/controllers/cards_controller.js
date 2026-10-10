import { Controller } from "@hotwired/stimulus";
import { t } from "controllers/i18n.js";

// How a browse page shows its listings: small cards, as many to a line as fit; big ones, one a
// line, every photo to slide through (slides_controller.js); or a list, a line each, the photo at
// its left. Picking one in the view menu switches at once and closes the menu; a cookie remembers
// the choice, so the pages after, and the next ones of this list, come the same way (browse.rs,
// Cards). Prona's pages keep theirs in a cookie of their own, which the page names (cards_cookie).
// The photos' sizes follow, for the browser to fetch the copies the cards now need (Cards::sizes).
export default class extends Controller {
  static targets = ["button", "menu"];
  static values = { small: String, big: String, list: String, cookie: { type: String, default: "cards" } };

  choose(event) {
    const layout = event.target.value;
    for (const other of ["big", "list"]) this.element.classList.toggle(`cards-${other}`, layout === other);
    const label = `${t("Pamja")}: ${event.target.closest("label").textContent.trim()}`;
    this.buttonTarget.setAttribute("aria-label", label);
    this.buttonTarget.title = label;
    document.cookie = `${this.cookieValue}=${layout}; path=/; max-age=31536000; samesite=lax`;
    const sizes = this[`${layout}Value`];
    for (const photo of this.element.querySelectorAll(".results .card-slide [sizes]")) photo.sizes = sizes;
    this.menuTarget.open = false;
  }
}
