import { Controller } from "@hotwired/stimulus";

// A big card's photos, side by side in a row that scrolls a photo at a time (page.css, scroll snap):
// a finger slides through them, and so does a mouse, pressed and moved: let go, the row settles on
// the photo nearest, or on the next one the way it was flicked, and the click that ends the drag
// opens no listing. The dots over the photo say which one shows, and under a mouse a click on one
// goes to its photo. The arrows step, round and round: the next from the last photo goes back to
// the first, the previous from the first to the last; each shows only while the mouse is near its
// edge of the photo (near-previous, near-next). A small card shows only its first photo, and none
// of this.

// How long a step from one photo to another takes, in milliseconds.
const STEP_MS = 200;
// How far a pressed mouse moves before it drags the photos, rather than clicking one.
const DRAG_FROM = 5;
// How fast a drag let go, in pixels a millisecond, goes on to the next photo however far it got.
const FLICK = 0.3;
// How near an edge of the photo the mouse brings that edge's arrow: a share of its width.
const EDGE = 0.25;

export default class extends Controller {
  static targets = ["track", "dots", "previous", "next"];

  connect() {
    const track = this.trackTarget;
    this.listeners = [
      [track, "pointerdown", (event) => this.grab(event)],
      [track, "pointermove", (event) => this.drag(event)],
      [window, "pointerup", (event) => this.drop(event)],
      [window, "pointercancel", (event) => this.drop(event)],
      [track, "dragstart", (event) => event.preventDefault()],
      [track, "click", (event) => this.swallow(event), { capture: true }],
      [this.dotsTarget, "click", (event) => this.pick(event)],
      [this.element, "pointermove", (event) => this.near(event)],
      [this.element, "pointerleave", () => this.far()],
    ];
    for (const [target, name, listener, options] of this.listeners) target.addEventListener(name, listener, options);
    this.update();
  }

  disconnect() {
    for (const [target, name, listener, options] of this.listeners ?? []) target.removeEventListener(name, listener, options);
    cancelAnimationFrame(this.frame);
  }

  previous() {
    this.go((this.index - 1 + this.count) % this.count);
  }

  next() {
    this.go((this.index + 1) % this.count);
  }

  // A dot clicked: its photo.
  pick(event) {
    const dot = event.target.closest("span");
    if (dot) this.go([...this.dotsTarget.children].indexOf(dot));
  }

  // To a photo, quicker than the browser's own smooth scrolling: STEP_MS, slowing at the end, the
  // snapping off meanwhile so that it doesn't pull the row back halfway.
  go(index) {
    const track = this.trackTarget;
    const from = track.scrollLeft;
    const to = Math.max(0, Math.min(track.scrollWidth - track.clientWidth, index * track.clientWidth));
    cancelAnimationFrame(this.frame);
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
      track.scrollLeft = to;
      track.style.removeProperty("scroll-snap-type");
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

  // A mouse pressed on the photos, and moved past DRAG_FROM: the row follows it, unsnapped.
  grab(event) {
    if (event.pointerType !== "mouse" || event.button !== 0 || !this.big) return;
    this.hold = { id: event.pointerId, x: event.clientX, scroll: this.trackTarget.scrollLeft, dragging: false, moves: [] };
  }

  drag(event) {
    const hold = this.hold;
    if (!hold || event.pointerId !== hold.id) return;
    const track = this.trackTarget;
    const moved = event.clientX - hold.x;
    if (!hold.dragging) {
      if (Math.abs(moved) < DRAG_FROM) return;
      hold.dragging = true;
      cancelAnimationFrame(this.frame);
      track.setPointerCapture(hold.id);
      track.style.scrollSnapType = "none";
      track.classList.add("is-dragging");
    }
    track.scrollLeft = hold.scroll - moved;
    hold.moves.push([event.timeStamp, event.clientX]);
    if (hold.moves.length > 5) hold.moves.shift();
  }

  // Let go: the photo nearest, or flicked, the next one the way the mouse was going.
  drop(event) {
    const hold = this.hold;
    if (!hold || event.pointerId !== hold.id) return;
    this.hold = null;
    if (!hold.dragging) return;
    const track = this.trackTarget;
    track.classList.remove("is-dragging");
    this.dragged = true;
    setTimeout(() => { this.dragged = false; });
    const first = hold.moves[0];
    const last = hold.moves.at(-1);
    const going = first && last[0] > first[0] && event.timeStamp - last[0] < 100;
    const speed = going ? (last[1] - first[1]) / (last[0] - first[0]) : 0;
    const at = track.clientWidth ? track.scrollLeft / track.clientWidth : 0;
    let index = Math.round(at);
    if (speed < -FLICK) index = Math.ceil(at);
    else if (speed > FLICK) index = Math.floor(at);
    this.go(index);
  }

  // The click that ends a drag, which would open the listing it ended on.
  swallow(event) {
    if (!this.dragged) return;
    event.preventDefault();
    event.stopPropagation();
    this.dragged = false;
  }

  // The mouse over the photo: the arrow of the edge it is near shows (page.css).
  near(event) {
    if (event.pointerType !== "mouse" || !this.big) return;
    const box = this.element.getBoundingClientRect();
    const zone = box.width * EDGE;
    this.element.classList.toggle("near-previous", event.clientX - box.left < zone);
    this.element.classList.toggle("near-next", box.right - event.clientX < zone);
  }

  far() {
    this.element.classList.remove("near-previous", "near-next");
  }

  // Which photo shows, as the row scrolls: its dot lights up. The arrows, there at either end as
  // they go round, come with the controller (the page has them hidden until it connects).
  update() {
    const index = this.index;
    [...this.dotsTarget.children].forEach((dot, at) => dot.classList.toggle("is-current", at === index));
    this.previousTarget.hidden = false;
    this.nextTarget.hidden = false;
  }

  // How many photos the card has: a dot each.
  get count() {
    return this.dotsTarget.children.length;
  }

  // Big cards only: a small card's photos don't slide, though it has this controller to slide them
  // once the view menu makes it big (cards_controller.js), and in a row the drag is the row's.
  get big() {
    return Boolean(this.element.closest(".cards-big .results"));
  }

  get index() {
    const track = this.trackTarget;
    return track.clientWidth ? Math.round(track.scrollLeft / track.clientWidth) : 0;
  }
}
