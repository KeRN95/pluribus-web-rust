import { Controller } from "@hotwired/stimulus";

// A listing's photos, one at a time. The strip scrolls and snaps under a finger by itself (CSS);
// this keeps the counter ("2 / 5") and the arrows (for a mouse) up to date, and moves the strip a
// photo when an arrow is clicked. Without it the strip still scrolls, and the counter says 1.

export default class extends Controller {
  static targets = ["track", "count", "previous", "next"];

  connect() {
    this.update = this.update.bind(this);
    this.trackTarget.addEventListener("scroll", this.update, { passive: true });
    this.update();
  }

  disconnect() {
    this.trackTarget.removeEventListener("scroll", this.update);
  }

  // The photo mostly in view, from 0.
  get index() {
    const track = this.trackTarget;
    const last = track.children.length - 1;
    return Math.min(last, Math.max(0, Math.round(track.scrollLeft / track.clientWidth)));
  }

  update() {
    const index = this.index;
    const total = this.trackTarget.children.length;
    this.countTarget.textContent = `${index + 1} / ${total}`;
    this.previousTarget.disabled = index === 0;
    this.nextTarget.disabled = index === total - 1;
  }

  previous() {
    this.show(this.index - 1);
  }

  next() {
    this.show(this.index + 1);
  }

  show(index) {
    const track = this.trackTarget;
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
    track.scrollTo({ left: index * track.clientWidth, behavior: still ? "auto" : "smooth" });
  }
}
