import { Controller } from "@hotwired/stimulus";

// The welcome page's row of the listings the visitor saw lately (home.html): "Pastro" forgets
// them, the cookie that keeps them (browse.rs, SEEN_COOKIE; application.js writes it) and the row,
// until the next listing's page.
export default class extends Controller {
  forget() {
    document.cookie = "shikuar=; path=/; max-age=0; samesite=lax";
    this.element.remove();
  }
}
