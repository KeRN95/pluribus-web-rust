import { Controller } from "@hotwired/stimulus";

// The welcome page's row of the newest listings (home.html). The server sends each listing as it
// is posted, no more than two a second (web/newest.rs), and Turbo puts it in front of the row
// (<turbo-stream-source>). Here, as Turbo does: the cards come in at least PACE apart, also when a
// proxy that held the stream back lets several through at once; a row at its start makes room for
// each, which comes in from the edge (.is-new in app.css), while a row slid along keeps the cards
// it shows where they are; and the row keeps the newest KEEP. The stream's address moves on to the
// newest card, for when Turbo shows the page again from its cache.
const KEEP = 10;
const PACE = 500;

export default class extends Controller {
  static targets = ["strip", "source"];

  // The cards waiting their turn, and when the next may come in.
  turns = Promise.resolve();
  nextAt = 0;

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
      // already, for its snapping, and the same again would be a card too far.
      strip.scrollLeft = scrolled + shown.offsetLeft - left;
    }
    while (strip.children.length > KEEP) strip.lastElementChild.remove();
    if (card?.dataset.cursor) this.sourceTarget.setAttribute("src", `/te-rejat?since=${card.dataset.cursor}`);
  }

  // Turbo keeps a copy of the page for going back: with no card halfway in.
  settle() {
    for (const card of this.stripTarget.querySelectorAll(".is-new")) card.classList.remove("is-new");
    this.stripTarget.style.removeProperty("scroll-snap-type");
  }
}
