import { Controller } from "@hotwired/stimulus";

// The site's header (base.html), which keeps to the screen's top as the page scrolls (base.css,
// position: sticky), gets out of the way of reading: scrolling down past it slides it up out of
// sight, and scrolling back up, however little, brings it back at once; at the page's top it is
// always there. It moves by a transform (base.css, is-hidden), so the page under it never shifts.
// A keyboard reaching one of its buttons brings it back too, and Turbo caching the page leaves it
// shown, so a page come back to doesn't open without it.
const TOLERANCE = 8; // px of scroll before it moves, so a finger that rests doesn't flicker it

export default class extends Controller {
  connect() {
    this.last = Math.max(0, scrollY);
    this.show();
    this.onScroll = () => {
      this.frame ??= requestAnimationFrame(() => {
        this.frame = null;
        this.follow();
      });
    };
    addEventListener("scroll", this.onScroll, { passive: true });
    this.element.addEventListener("focusin", this.show);
  }

  follow() {
    const y = Math.max(0, scrollY);
    const moved = y - this.last;
    if (y <= this.element.offsetHeight) {
      this.show();
    } else if (moved > TOLERANCE) {
      this.hide();
    } else if (moved < -TOLERANCE) {
      this.show();
    } else {
      return; // too little to tell: counted on with the next
    }
    this.last = y;
  }

  show = () => this.element.classList.remove("is-hidden");

  hide = () => this.element.classList.add("is-hidden");

  disconnect() {
    removeEventListener("scroll", this.onScroll);
    cancelAnimationFrame(this.frame);
    this.frame = null;
    this.element.removeEventListener("focusin", this.show);
    this.show();
  }
}
