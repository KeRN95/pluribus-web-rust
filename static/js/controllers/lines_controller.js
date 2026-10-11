import { Controller } from "@hotwired/stimulus";

// A listing's description in the list (page.css, .cards-list .card-description): as many whole lines
// as the room under its details holds, the last ending in "…", as the user asked on 2026-10-11 (two
// at most until then, one on a phone, which is what it shows without scripts). The room is the
// description's own box, which the card's grid stretches over what the title, the details and the
// place's line leave it; it is measured again whenever that box changes size: the view menu making
// the cards a list, the text's size, the page's, a narrower screen wrapping the title.
export default class extends Controller {
  static targets = ["text"];

  // Made before the targets connect, which Stimulus does before connect().
  initialize() {
    this.watcher = new ResizeObserver((entries) => {
      for (const entry of entries) this.fit(entry.target);
    });
  }

  disconnect() {
    this.watcher.disconnect();
  }

  textTargetConnected(text) {
    this.watcher.observe(text);
  }

  textTargetDisconnected(text) {
    this.watcher.unobserve(text);
  }

  fit(text) {
    const style = getComputedStyle(text);
    if (style.display === "none") return;
    const room = text.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
    // A hair under a whole line still takes it, as the browser rounds the box.
    const lines = Math.floor((room + 0.5) / parseFloat(style.lineHeight));
    text.style.setProperty("--lines", String(Math.max(lines, 1)));
    text.toggleAttribute("data-no-room", lines < 1);
  }
}
