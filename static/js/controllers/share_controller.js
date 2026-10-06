import { Controller } from "@hotwired/stimulus";

// Opens the phone's share sheet for a listing, or copies its link where there is none.
export default class extends Controller {
  static targets = ["label"];
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
    await navigator.clipboard.writeText(url);
    this.labelTarget.textContent = "Linku u kopjua";
  }
}
