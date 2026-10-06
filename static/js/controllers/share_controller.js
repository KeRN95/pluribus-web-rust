import { Controller } from "@hotwired/stimulus";

// Opens the phone's share sheet for a listing, or copies its link where there is none, and says
// so for a moment as the copy button beside the number does: a check, "Linku u kopjua".
export default class extends Controller {
  static targets = ["button"];
  static values = { url: String, title: String };

  async share() {
    const url = new URL(this.urlValue, window.location.origin).href;
    if (navigator.share) {
      try {
        await navigator.share({ title: this.titleValue, url });
      } catch {
        // The visitor closed the share sheet.
      }
      return;
    }
    try {
      await navigator.clipboard.writeText(url);
    } catch {
      return;
    }
    const button = this.buttonTarget;
    button.classList.add("is-copied");
    button.setAttribute("aria-label", "Linku u kopjua");
    clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      button.classList.remove("is-copied");
      button.setAttribute("aria-label", button.title);
    }, 1500);
  }

  disconnect() {
    clearTimeout(this.timer);
  }
}
