import { Controller } from "@hotwired/stimulus";
import { signedIn } from "controllers/signed_in.js";
import { t, tf } from "controllers/i18n.js";

// The header's button for the conversations (base.html), or the welcome page's at the end of its
// categories (home.html): how many have messages the visitor hasn't read, in the accent at its
// top right (messages.css, .inbox-count), asked for with each page (GET /mesazhet/numri) and again once a conversation's page
// has read some (conversation_controller.js), and said to screen readers. Someone not signed in
// has none, and the page doesn't ask (signed_in.js).
export default class extends Controller {
  static targets = ["count"];

  connect() {
    this.ask = this.ask.bind(this);
    this.ask();
    addEventListener("messages:read", this.ask);
  }

  disconnect() {
    removeEventListener("messages:read", this.ask);
  }

  async ask() {
    if (!signedIn()) {
      this.show(0);
      return;
    }
    try {
      const response = await fetch("/mesazhet/numri", { headers: { Accept: "application/json" }, cache: "no-store" });
      if (response.ok) this.show((await response.json()).unread);
    } catch {}
  }

  show(unread) {
    const label = unread > 0 ? tf("Mesazhet: {} të palexuara", unread) : t("Mesazhet");
    this.countTarget.textContent = unread > 9 ? "9+" : String(unread);
    this.countTarget.hidden = !(unread > 0);
    this.element.setAttribute("aria-label", label);
    this.element.title = label;
  }
}
