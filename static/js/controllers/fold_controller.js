import { Controller } from "@hotwired/stimulus";

// A listing's description folded (listing.html, .description-fold): longer than a dozen lines, it
// shows that many, fading out at the foot, with "Shfaq të gjithë përshkrimin" under it; a tap
// unfolds it for good. Shorter, it shows whole and the button stays hidden. Without scripts it
// shows whole. The fold is set here rather than in the page, so that nothing is hidden before
// its length is known.
export default class extends Controller {
  static targets = ["text", "button"];

  connect() {
    this.element.classList.add("is-folded");
    if (this.textTarget.scrollHeight > this.textTarget.clientHeight + 1) {
      this.buttonTarget.hidden = false;
    } else {
      this.element.classList.remove("is-folded");
    }
  }

  open() {
    this.element.classList.remove("is-folded");
    this.buttonTarget.hidden = true;
  }
}
