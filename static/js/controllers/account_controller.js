import { Controller } from "@hotwired/stimulus";
import { signedIn } from "controllers/signed_in.js";

// The account's round button at the top bar's right end (base.html): a dot while a notification
// about one of the visitor's own listings is unread, one about to expire or expired (expiry.rs),
// which no other button shows (the heart has the prices dropped and the saved searches, the speech
// bubble the messages), asked for with each page (GET /lajmerimet/numri) and said to screen
// readers; not on /lajmerimet, which shows them. Someone not signed in has none, and the page
// doesn't ask (signed_in.js).
export default class extends Controller {
  connect() {
    this.ask();
  }

  async ask() {
    if (!signedIn()) {
      this.show(0);
      return;
    }
    try {
      const response = await fetch("/lajmerimet/numri", { headers: { Accept: "application/json" }, cache: "no-store" });
      if (response.ok) this.show((await response.json()).own);
    } catch {}
  }

  show(own) {
    const on = own > 0 && !document.querySelector(".notification-list");
    const label = on ? `Llogaria: ${own === 1 ? "1 lajmërim" : `${own} lajmërime`} për njoftimet e tua` : "Llogaria";
    this.element.classList.toggle("has-news", on);
    this.element.setAttribute("aria-label", label);
    this.element.title = label;
  }
}
