import { Controller } from "@hotwired/stimulus";

// The welcome page's row of the newest listings (home.html). The server sends each listing as it
// is posted, no more than two a second (web/newest.rs), and Turbo puts it in front of the row
// (<turbo-stream-source>). Here, as Turbo does: the cards come in one by one, at least PACE apart,
// also when a proxy that held the stream back lets several through at once, each sliding in from
// the edge (.is-new in app.css), and the row keeps the newest KEEP. While the row is slid along,
// under a finger or still moving, they wait instead, so that nothing moves under the visitor: a
// fade over the row's start says how many have come, and a tap on it goes back to the start,
// where they come in. The stream's address moves on to the newest card in the row, for when Turbo
// shows the page again from its cache. A mouse moves the row too: its wheel turns it, and it drags
// it as a finger slides it.
const KEEP = 30;
const PACE = 500;
// How far a pressed mouse moves before it drags the row, rather than clicking a card.
const DRAG_FROM = 5;
// How long the row is still before cards come in again, in milliseconds.
const STILL = 200;

export default class extends Controller {
  static targets = ["strip", "source", "waiting", "count"];

  // The cards that came and wait their turn, the oldest first, each with what tells Turbo it is
  // done with; how many more came than wait (only the newest KEEP do); when the next may come in.
  queue = [];
  dropped = 0;
  nextAt = 0;
  // A finger or the mouse is down on the row; the row is moving.
  touching = false;
  moving = false;
  // Where the wheel is taking the row while it turns, and the mouse while it is pressed on it.
  goal = null;
  hold = null;

  disconnect() {
    for (const card of this.queue) card.done();
    this.queue = [];
    this.dropped = 0;
  }

  // turbo:before-stream-render, for every Turbo Stream on the page: the row's wait their turn.
  arrive(event) {
    if (event.target.getAttribute("target") !== this.stripTarget.id) return;
    const render = event.detail.render;
    event.detail.render = (stream) =>
      new Promise((done) => {
        this.queue.push({ stream, render, done });
        while (this.queue.length > KEEP) {
          this.queue.shift().done();
          this.dropped += 1;
        }
        this.tell();
        this.next();
      });
  }

  // Cards come in only while the row is at its start and still.
  get free() {
    return this.stripTarget.scrollLeft < 8 && !this.touching && !this.moving && !this.hold;
  }

  // The cards that wait come in one by one, PACE apart, for as long as the row is free.
  async next() {
    if (this.placing) return;
    this.placing = true;
    try {
      while (this.queue.length && this.free) {
        const wait = this.nextAt - performance.now();
        if (wait > 0) {
          await new Promise((resolve) => setTimeout(resolve, wait));
          continue;
        }
        const { stream, render, done } = this.queue.shift();
        await this.place(stream, render);
        done();
        this.nextAt = performance.now() + PACE;
      }
      if (!this.queue.length) this.dropped = 0;
    } finally {
      this.placing = false;
      this.tell();
    }
  }

  // A card in front of the row, at its start: it comes in from the edge, the others moving along.
  async place(stream, render) {
    const strip = this.stripTarget;
    // Snapping would hold on to the card that was first; the new one is to show.
    strip.style.scrollSnapType = "none";
    await render(stream);
    const card = strip.firstElementChild;
    strip.scrollLeft = 0;
    card?.classList.add("is-new");
    const snap = () => strip.style.removeProperty("scroll-snap-type");
    if (card && getComputedStyle(card).animationName !== "none") card.addEventListener("animationend", snap, { once: true });
    else snap();
    while (strip.children.length > KEEP) strip.lastElementChild.remove();
    if (card?.dataset.cursor) this.sourceTarget.setAttribute("src", `/te-rejat?since=${card.dataset.cursor}`);
  }

  // The fade over the row's start, while it is slid along and cards wait: how many have come.
  tell() {
    const count = this.queue.length + this.dropped;
    this.waitingTarget.hidden = !count || this.stripTarget.scrollLeft < 8;
    if (!count) return;
    this.countTarget.textContent = count === 1 ? "1 i ri" : `${count} të reja`;
    const listings = count === 1 ? "1 njoftim i ri" : `${count} njoftime të reja`;
    this.waitingTarget.querySelector("button").setAttribute("aria-label", `${listings}: kthehu në fillim`);
  }

  // A tap on the fade: back to the row's start, where the cards that wait come in. It counts as the
  // row moving, should the browser not say so (scrolled), so that they come in once it is there.
  backToStart() {
    this.stripTarget.scrollTo({ left: 0, behavior: this.smooth });
    this.scrolled();
  }

  // scroll: while the row moves, cards wait; once it has been still for STILL, they may come in.
  scrolled() {
    this.moving = true;
    this.tell();
    clearTimeout(this.stillTimer);
    this.stillTimer = setTimeout(() => {
      this.moving = false;
      this.next();
    }, STILL);
  }

  // A mouse's wheel turns the row sideways, smoothly, while it has further to go that way; at its
  // ends the page scrolls on. Sideways wheels and trackpads scroll it as they always do.
  wheel(event) {
    if (event.ctrlKey || Math.abs(event.deltaX) >= Math.abs(event.deltaY)) return;
    const strip = this.stripTarget;
    const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 40 : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? strip.clientWidth : 1;
    const from = this.goal ?? strip.scrollLeft;
    const goal = Math.max(0, Math.min(strip.scrollWidth - strip.clientWidth, from + event.deltaY * unit));
    if (Math.abs(goal - from) < 1) return;
    event.preventDefault();
    this.goal = goal;
    strip.scrollTo({ left: goal, behavior: this.smooth });
    clearTimeout(this.wheelStop);
    this.wheelStop = setTimeout(() => { this.goal = null; }, 300);
  }

  // A finger or the mouse down on the row: cards wait until it is off again. A mouse pressed on the
  // row drags it along once it has moved DRAG_FROM; let go, the row carries on a little the way it
  // was going, and the click that ends the drag opens no card.
  grab(event) {
    this.touching = true;
    if (event.pointerType !== "mouse" || event.button !== 0) return;
    this.hold = { id: event.pointerId, x: event.clientX, scroll: this.stripTarget.scrollLeft, dragging: false, moves: [] };
  }

  drag(event) {
    const hold = this.hold;
    if (!hold || event.pointerId !== hold.id) return;
    const moved = event.clientX - hold.x;
    if (!hold.dragging) {
      if (Math.abs(moved) < DRAG_FROM) return;
      hold.dragging = true;
      this.goal = null;
      this.stripTarget.setPointerCapture(hold.id);
      this.stripTarget.classList.add("is-dragging");
    }
    this.stripTarget.scrollLeft = hold.scroll - moved;
    hold.moves.push([event.timeStamp, event.clientX]);
    if (hold.moves.length > 5) hold.moves.shift();
  }

  // pointerup and pointercancel, anywhere on the page.
  drop(event) {
    this.touching = false;
    const hold = this.hold;
    if (hold && event.pointerId === hold.id) {
      this.hold = null;
      if (hold.dragging) this.fling(hold, event);
    }
    this.next();
  }

  fling(hold, event) {
    this.stripTarget.classList.remove("is-dragging");
    this.dragged = true;
    setTimeout(() => { this.dragged = false; });
    // How fast it was going when let go, in pixels a millisecond, if it hadn't stopped first.
    const first = hold.moves[0];
    const last = hold.moves.at(-1);
    const going = first && last[0] > first[0] && event.timeStamp - last[0] < 100;
    const speed = going ? (last[1] - first[1]) / (last[0] - first[0]) : 0;
    if (Math.abs(speed) > 0.3) this.stripTarget.scrollBy({ left: -speed * 250, behavior: this.smooth });
  }

  // The click that ends a drag, which would open the card it ended on.
  swallow(event) {
    if (!this.dragged) return;
    event.preventDefault();
    event.stopPropagation();
    this.dragged = false;
  }

  // The browser's own dragging of a photo or a link, which would take the mouse from the row.
  cancel(event) {
    event.preventDefault();
  }

  get smooth() {
    return matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth";
  }

  // Turbo keeps a copy of the page for going back: with no card halfway in, and no fade.
  settle() {
    for (const card of this.stripTarget.querySelectorAll(".is-new")) card.classList.remove("is-new");
    this.stripTarget.style.removeProperty("scroll-snap-type");
    this.waitingTarget.hidden = true;
  }
}
