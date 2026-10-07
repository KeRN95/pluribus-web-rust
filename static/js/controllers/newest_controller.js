import { Controller } from "@hotwired/stimulus";

// The welcome page's row of the newest listings (home.html). The server sends each listing as it
// is posted, no more than two a second (web/newest.rs), and Turbo puts it in front of the row
// (<turbo-stream-source>). Here, as Turbo does: the cards come in at least PACE apart, also when a
// proxy that held the stream back lets several through at once; a row at its start makes room for
// each, which comes in from the edge (.is-new in app.css), while a row slid along keeps the cards
// it shows where they are; and the row keeps the newest KEEP. The stream's address moves on to the
// newest card, for when Turbo shows the page again from its cache. A mouse moves the row too: its
// wheel turns it, and it drags it as a finger slides it.
const KEEP = 10;
const PACE = 500;
// How far a pressed mouse moves before it drags the row, rather than clicking a card.
const DRAG_FROM = 5;

export default class extends Controller {
  static targets = ["strip", "source"];

  // The cards waiting their turn, and when the next may come in.
  turns = Promise.resolve();
  nextAt = 0;
  // Where the wheel is taking the row while it turns, and the mouse while it is pressed on it.
  goal = null;
  hold = null;

  // turbo:before-stream-render, for every Turbo Stream on the page: only the row's are changed.
  arrive(event) {
    if (event.target.getAttribute("target") !== this.stripTarget.id) return;
    const render = event.detail.render;
    event.detail.render = (stream) => this.inTurn(() => this.place(stream, render));
  }

  inTurn(step) {
    const turn = this.turns.then(async () => {
      const wait = this.nextAt - performance.now();
      if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
      await step();
      this.nextAt = performance.now() + PACE;
    });
    this.turns = turn.catch(() => {});
    return turn;
  }

  async place(stream, render) {
    const strip = this.stripTarget;
    const scrolled = strip.scrollLeft;
    const atStart = scrolled < 8;
    // The first card in view, and where it is, to keep it there.
    const shown = [...strip.children].find((card) => card.offsetLeft + card.offsetWidth > scrolled);
    const left = shown?.offsetLeft;
    // Snapping would hold on to the card that was first; at the start, the new one is to show.
    if (atStart) strip.style.scrollSnapType = "none";
    await render(stream);
    const card = strip.firstElementChild;
    if (atStart) {
      strip.scrollLeft = 0;
      card?.classList.add("is-new");
      const snap = () => strip.style.removeProperty("scroll-snap-type");
      if (card && getComputedStyle(card).animationName !== "none") card.addEventListener("animationend", snap, { once: true });
      else snap();
    } else if (shown?.isConnected) {
      // Where it was, from where the row was slid to: the browser may have moved the row along
      // already, for its snapping, and the same again would be a card too far. The wheel and a
      // drag go on from there.
      const shift = shown.offsetLeft - left;
      strip.scrollLeft = scrolled + shift;
      if (this.goal !== null) this.goal += shift;
      if (this.hold) this.hold.scroll += shift;
    }
    while (strip.children.length > KEEP) strip.lastElementChild.remove();
    if (card?.dataset.cursor) this.sourceTarget.setAttribute("src", `/te-rejat?since=${card.dataset.cursor}`);
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

  // A mouse pressed on the row drags it along once it has moved DRAG_FROM; let go, the row carries
  // on a little the way it was going, and the click that ends the drag opens no card.
  grab(event) {
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

  drop(event) {
    const hold = this.hold;
    if (!hold || event.pointerId !== hold.id) return;
    this.hold = null;
    if (!hold.dragging) return;
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

  // Turbo keeps a copy of the page for going back: with no card halfway in.
  settle() {
    for (const card of this.stripTarget.querySelectorAll(".is-new")) card.classList.remove("is-new");
    this.stripTarget.style.removeProperty("scroll-snap-type");
  }
}
