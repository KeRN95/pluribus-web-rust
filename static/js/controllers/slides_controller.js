import { Controller } from "@hotwired/stimulus";

// Photos side by side in a row that scrolls a photo at a time (scroll snap): a big card's (page.css)
// and a listing's on its page (listing.css, .gallery). A finger slides through them, and so does a
// mouse, pressed and moved: let go, the row settles on the photo nearest, or on the next one the
// way it was flicked, and the click that ends the drag opens no listing. The dots over the photo
// say which one shows, and under a mouse a click on one goes to its photo; the listing's counter
// ("2 / 5") says it too. The arrows step, round and round: the next from the last photo goes back
// to the first, the previous from the first to the last; each shows only while the mouse is near
// its edge of the photo (near-previous, near-next). A small card shows only its first photo, and
// none of this. A listing's page has a filmstrip of its photos over their foot too (listing.html,
// .gallery-thumbs), hidden until its counter is clicked, which hides it again: each thumbnail goes
// to its photo, the one that shows is marked and kept in view as the photos slide, and under a
// mouse an arrow at either end steps the strip by most of its width, each there while the strip
// can go that way.

// How long a step from one photo to another takes, in milliseconds.
const STEP_MS = 200;
// How far a pressed mouse moves before it drags the photos, rather than clicking one.
const DRAG_FROM = 5;
// How fast a drag let go, in pixels a millisecond, goes on to the next photo however far it got.
const FLICK = 0.3;
// How near an edge of the photo the mouse brings that edge's arrow: a share of its width.
const EDGE = 0.25;

export default class extends Controller {
  static targets = ["track", "dots", "count", "previous", "next", "filmstrip", "thumbs", "thumbsPrevious", "thumbsNext"];

  connect() {
    const track = this.trackTarget;
    this.listeners = [
      [track, "pointerdown", (event) => this.grab(event)],
      [track, "pointermove", (event) => this.drag(event)],
      [window, "pointerup", (event) => this.drop(event)],
      [window, "pointercancel", (event) => this.drop(event)],
      [track, "dragstart", (event) => event.preventDefault()],
      [track, "click", (event) => this.swallow(event), { capture: true }],
      [this.element, "pointermove", (event) => this.near(event)],
      [this.element, "pointerleave", () => this.far()],
    ];
    if (this.hasDotsTarget) this.listeners.push([this.dotsTarget, "click", (event) => this.pick(event)]);
    for (const [target, name, listener, options] of this.listeners) target.addEventListener(name, listener, options);
    this.update();
    // The filmstrip's arrows follow its width, once it has one and as it changes (a page laid out
    // before it shows has none yet).
    if (this.hasThumbsTarget) {
      this.watcher = new ResizeObserver(() => this.scrolled());
      this.watcher.observe(this.thumbsTarget);
    }
  }

  disconnect() {
    for (const [target, name, listener, options] of this.listeners ?? []) target.removeEventListener(name, listener, options);
    this.watcher?.disconnect();
    cancelAnimationFrame(this.frame);
  }

  previous() {
    this.go((this.index - 1 + this.count) % this.count);
  }

  next() {
    this.go((this.index + 1) % this.count);
  }

  // A dot, or a thumbnail in the filmstrip, clicked: its photo, by its place among its fellows.
  pick(event) {
    const item = event.target.closest(".gallery-thumb, span");
    if (item) this.go([...item.parentElement.children].indexOf(item));
  }

  // One of a car's mosaic's tiles (listing.html, .mosaic-side): the slider goes to its photo; the
  // one that says how many more there are shows the filmstrip of them all as well.
  tile({ params: { index, more } }) {
    this.go(index);
    if (!more || !this.hasFilmstripTarget || !this.filmstripTarget.hidden) return;
    this.filmstripTarget.hidden = false;
    this.filmstripTarget.parentElement.classList.add("thumbs-open");
    this.element.querySelector(".gallery-count")?.setAttribute("aria-expanded", "true");
    this.update();
  }

  // The counter clicked (the button over the photo): the filmstrip shows, the photo that shows
  // marked in it and its thumbnail brought into view, or hides again; the button says which.
  toggleThumbs(event) {
    const filmstrip = this.filmstripTarget;
    filmstrip.hidden = !filmstrip.hidden;
    // The gallery's dots give way to it (listing.css, .gallery.thumbs-open).
    filmstrip.parentElement.classList.toggle("thumbs-open", !filmstrip.hidden);
    event.currentTarget.setAttribute("aria-expanded", String(!filmstrip.hidden));
    if (!filmstrip.hidden) this.update();
  }

  // The filmstrip's arrows: most of its width back, or on, smoothly unless motion is unwanted.
  thumbsBack() {
    this.stepThumbs(-1);
  }

  thumbsOn() {
    this.stepThumbs(1);
  }

  stepThumbs(way) {
    const strip = this.thumbsTarget;
    strip.scrollBy({ left: way * strip.clientWidth * 0.8, behavior: this.quick ? "auto" : "smooth" });
  }

  // The filmstrip scrolled, or its width changed: each arrow shows while the strip can go its way,
  // so a strip whose thumbnails all fit has none.
  scrolled() {
    if (!this.hasThumbsTarget) return;
    const strip = this.thumbsTarget;
    this.thumbsPreviousTarget.hidden = strip.scrollLeft <= 1;
    this.thumbsNextTarget.hidden = strip.scrollLeft + strip.clientWidth >= strip.scrollWidth - 1;
  }

  // The thumbnail of the photo that shows, kept in view: the strip slides just far enough, and
  // the page itself not at all.
  show(thumb) {
    const strip = this.thumbsTarget;
    const left = thumb.offsetLeft - strip.clientLeft;
    const behavior = this.quick ? "auto" : "smooth";
    if (left < strip.scrollLeft) strip.scrollTo({ left, behavior });
    else if (left + thumb.offsetWidth > strip.scrollLeft + strip.clientWidth) strip.scrollTo({ left: left + thumb.offsetWidth - strip.clientWidth, behavior });
  }

  get quick() {
    return matchMedia("(prefers-reduced-motion: reduce)").matches;
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
    if (event.pointerType !== "mouse" || event.button !== 0 || !this.sliding) return;
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
    // The pointer moves in the screen's pixels, the photos scroll in the page's (data-page-size).
    track.scrollLeft = hold.scroll - moved / (track.currentCSSZoom || 1);
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

  // The mouse over the photo: the arrow of the edge it is near shows (page.css, listing.css). The
  // photo's own box, not the controller's, which on a listing's page holds the filmstrip too.
  near(event) {
    if (event.pointerType !== "mouse" || !this.sliding) return;
    const box = this.trackTarget.getBoundingClientRect();
    const zone = box.width * EDGE;
    this.element.classList.toggle("near-previous", event.clientX - box.left < zone);
    this.element.classList.toggle("near-next", box.right - event.clientX < zone);
  }

  far() {
    this.element.classList.remove("near-previous", "near-next");
  }

  // Which photo shows, as the row scrolls: its dot lights up, its thumbnail in the filmstrip is
  // marked and kept in view, and the counter says its number. The arrows, there at either end as
  // they go round, come with the controller (the page has them hidden until it connects).
  update() {
    const index = this.index;
    if (this.hasDotsTarget) [...this.dotsTarget.children].forEach((dot, at) => dot.classList.toggle("is-current", at === index));
    if (this.hasThumbsTarget) {
      const thumbs = [...this.thumbsTarget.children];
      thumbs.forEach((thumb, at) => {
        thumb.classList.toggle("is-current", at === index);
        if (at === index) thumb.setAttribute("aria-current", "true");
        else thumb.removeAttribute("aria-current");
      });
      if (thumbs[index]) this.show(thumbs[index]);
    }
    if (this.hasCountTarget) this.countTarget.textContent = `${index + 1} / ${this.count}`;
    this.previousTarget.hidden = false;
    this.nextTarget.hidden = false;
  }

  // How many photos there are: the row's.
  get count() {
    return this.trackTarget.children.length;
  }

  // A listing's photos always; a card's on a big card only (the results' once the view menu makes
  // them big, cards_controller.js; "Më të klikuarat"'s two always): a small card's photos don't
  // slide, though it has this controller to slide them once it is big, and in a row the drag is
  // the row's.
  get sliding() {
    return !this.element.closest(".card") || Boolean(this.element.closest(".cards-big .results, .big-cards"));
  }

  get index() {
    const track = this.trackTarget;
    return track.clientWidth ? Math.round(track.scrollLeft / track.clientWidth) : 0;
  }
}
