import { Controller } from "@hotwired/stimulus";

// A listing's path and the three step buttons at its right on one line (listing.html,
// .listing-tags). On a phone the path's pills are their icons alone but the page's own, which keeps
// its name; a long one ("Mbushës për makina elektrike") left the step buttons no room and pushed
// them down to a line of their own. As the page shows, and again as the screen's width changes or
// the fonts come in, the buttons are tried on the pill's line with its name, and where they fall
// under it the pill goes to its icon alone as well (.is-tight, listing.css), its name still the
// link's, for screen readers. Without scripts the buttons take the line under.
export default class extends Controller {
  connect() {
    this.fit();
    this.resized = () => this.fit();
    window.addEventListener("resize", this.resized);
    document.fonts?.ready.then(() => {
      if (this.element.isConnected) this.fit();
    });
  }

  disconnect() {
    window.removeEventListener("resize", this.resized);
    this.element.classList.remove("is-tight");
  }

  fit() {
    const path = this.element.querySelector(".listing-path");
    const steps = this.element.querySelector(".listing-steps");
    if (!path || !steps) return;
    this.element.classList.remove("is-tight");
    const under = steps.getBoundingClientRect().top >= path.getBoundingClientRect().bottom - 1;
    this.element.classList.toggle("is-tight", under);
  }
}
