import { Controller } from "@hotwired/stimulus";

// A row of cards that slides sideways (card_row.html: "Më të klikuarat", a seller's
// other listings), moved by a mouse as a finger moves it: pressed on the row and moved, it drags
// the row along; let go, the row carries on a little the way it was going, and the click that ends
// the drag opens no card. Not the wheel: these rows sit in long pages, which it scrolls; a
// trackpad's sideways swipe slides them as ever.

// How far a pressed mouse moves before it drags the row, rather than clicking a card.
const DRAG_FROM = 5;

export default class extends Controller {
  connect() {
    this.row = this.element.querySelector(".card-row");
    if (!this.row) return;
    this.listeners = {
      pointerdown: [this.row, (event) => this.grab(event)],
      pointermove: [this.row, (event) => this.drag(event)],
      pointerup: [window, (event) => this.drop(event)],
      pointercancel: [window, (event) => this.drop(event)],
      dragstart: [this.row, (event) => event.preventDefault()],
      click: [this.row, (event) => this.swallow(event), { capture: true }],
    };
    for (const [name, [target, listener, options]] of Object.entries(this.listeners)) target.addEventListener(name, listener, options);
  }

  disconnect() {
    for (const [name, [target, listener, options]] of Object.entries(this.listeners ?? {})) target.removeEventListener(name, listener, options);
  }

  grab(event) {
    if (event.pointerType !== "mouse" || event.button !== 0) return;
    this.hold = { id: event.pointerId, x: event.clientX, scroll: this.row.scrollLeft, dragging: false, moves: [] };
  }

  drag(event) {
    const hold = this.hold;
    if (!hold || event.pointerId !== hold.id) return;
    const moved = event.clientX - hold.x;
    if (!hold.dragging) {
      if (Math.abs(moved) < DRAG_FROM) return;
      hold.dragging = true;
      this.row.setPointerCapture(hold.id);
      this.row.classList.add("is-dragging");
    }
    // The pointer moves in the screen's pixels, the row scrolls in the page's (data-page-size).
    this.row.scrollLeft = hold.scroll - moved / (this.row.currentCSSZoom || 1);
    hold.moves.push([event.timeStamp, event.clientX]);
    if (hold.moves.length > 5) hold.moves.shift();
  }

  drop(event) {
    const hold = this.hold;
    if (!hold || event.pointerId !== hold.id) return;
    this.hold = null;
    if (!hold.dragging) return;
    this.row.classList.remove("is-dragging");
    this.dragged = true;
    setTimeout(() => { this.dragged = false; });
    // How fast it was going when let go, in pixels a millisecond, if it hadn't stopped first.
    const first = hold.moves[0];
    const last = hold.moves.at(-1);
    const going = first && last[0] > first[0] && event.timeStamp - last[0] < 100;
    const speed = going ? (last[1] - first[1]) / (last[0] - first[0]) : 0;
    if (Math.abs(speed) > 0.3) this.row.scrollBy({ left: (-speed * 250) / (this.row.currentCSSZoom || 1), behavior: this.smooth });
  }

  // The click that ends a drag, which would open the card it ended on.
  swallow(event) {
    if (!this.dragged) return;
    event.preventDefault();
    event.stopPropagation();
    this.dragged = false;
  }

  get smooth() {
    return matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth";
  }
}
