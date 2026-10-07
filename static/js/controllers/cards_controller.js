import { Controller } from "@hotwired/stimulus";

// How a browse page shows its listings: small cards, as many to a line as fit, or big ones, one a
// line, every photo to slide through (slides_controller.js). The button switches at once; a cookie
// remembers the choice, so the pages after, and the next ones of this list, come the same way
// (browse.rs, big_cards). Prona's pages keep theirs in a cookie of their own, which the page names
// (cards_cookie). The photos' sizes follow, for the browser to fetch the larger copies a big card
// needs (ListingPage::sizes).
export default class extends Controller {
  static targets = ["button"];
  static values = { small: String, big: String, cookie: { type: String, default: "cards" } };

  toggle() {
    const big = !this.element.classList.contains("cards-big");
    this.element.classList.toggle("cards-big", big);
    this.buttonTarget.setAttribute("aria-pressed", String(big));
    document.cookie = `${this.cookieValue}=${big ? "big" : "small"}; path=/; max-age=31536000; samesite=lax`;
    for (const photo of this.element.querySelectorAll(".results .card-photo[sizes]")) {
      photo.sizes = big ? this.bigValue : this.smallValue;
    }
  }
}
