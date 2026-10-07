import { Controller } from "@hotwired/stimulus";

// A big card's photos, side by side in a row that scrolls a photo at a time (page.css, scroll snap):
// a finger slides through them, and under a pointer the arrows step; the dots over the photo say
// which one shows. A small card shows only its first photo, and none of this.

// How long an arrow's step from one photo to the next takes, in milliseconds.
const STEP_MS = 200;

export default class extends Controller {
  static targets = ["track", "dots", "previous", "next"];

  connect() {
    this.update();
  }

  previous() {
    this.step(-1);
  }

  next() {
    this.step(1);
  }

  // An arrow's step, quicker than the browser's own smooth scrolling: STEP_MS, slowing at the end,
  // the snapping off meanwhile so that it doesn't pull the row back halfway.
  step(by) {
    const track = this.trackTarget;
    const from = track.scrollLeft;
    const to = Math.max(0, Math.min(track.scrollWidth - track.clientWidth, Math.round(from / track.clientWidth + by) * track.clientWidth));
    cancelAnimationFrame(this.frame);
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
      track.scrollLeft = to;
      return;
    }
    track.style.scrollSnapType = "none";
    const start = performance.now();
    const move = (now) => {
      const done = Math.min(1, (now - start) / STEP_MS);
      track.scrollLeft = from + (to - from) * (1 - (1 - done) ** 3);
      if (done < 1) this.frame = requestAnimationFrame(move);
      else track.style.removeProperty("scroll-snap-type");
    };
    this.frame = requestAnimationFrame(move);
  }

  // Which photo shows, as the row scrolls: its dot lights up, and an arrow goes at either end.
  update() {
    const track = this.trackTarget;
    const dots = [...this.dotsTarget.children];
    const index = track.clientWidth ? Math.round(track.scrollLeft / track.clientWidth) : 0;
    dots.forEach((dot, at) => dot.classList.toggle("is-current", at === index));
    this.previousTarget.hidden = index <= 0;
    this.nextTarget.hidden = index >= dots.length - 1;
  }
}
