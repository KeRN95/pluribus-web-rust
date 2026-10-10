import { Controller } from "@hotwired/stimulus";
import { t } from "controllers/i18n.js";

// Copies a text, the seller's number or address, and says so for a moment as Claude's copy
// buttons do: the icon turns into a check, and the button's name into "U kopjua".
export default class extends Controller {
  static values = { text: String };

  async copy() {
    try {
      await navigator.clipboard.writeText(this.textValue);
    } catch {
      return;
    }
    const name = this.element.title;
    this.element.classList.add("is-copied");
    this.element.setAttribute("aria-label", t("U kopjua"));
    clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.element.classList.remove("is-copied");
      this.element.setAttribute("aria-label", name);
    }, 1500);
  }

  disconnect() {
    clearTimeout(this.timer);
  }
}
