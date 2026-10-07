import { Controller } from "@hotwired/stimulus";

// A big card's photos, side by side in a row that scrolls a photo at a time (app.css, scroll snap):
// a finger slides through them, and under a pointer the arrows step; the dots over the photo say
// which one shows. A small card shows only its first photo, and none of this.
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

  step(by) {
    this.trackTarget.scrollBy({ left: by * this.trackTarget.clientWidth, behavior: "smooth" });
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
