import { Controller } from "@hotwired/stimulus";

// A listing's message sheet (listing.html). A message button, signed in, loads the conversation
// into the sheet's frame (data-turbo-frame) and opens it. On a phone it slides up from the screen's
// bottom to its middle (messages.css), the page under it held still; its bar at the top, or its
// head, dragged down past a quarter of its height or flicked down closes it, sliding the rest of
// the way, and let go sooner it goes back up. From 720 px up it is a popup card from the button
// instead: under it, or over it where the screen has more room there, as tall as that room allows,
// the page brought along to show it whole; a click outside it closes it, as a menu's would.
// The box to write in takes the focus once the conversation is in, where there is a mouse; on a
// phone the keyboard waits for a tap in it. "Mbyll", Escape, or Turbo caching the page closes it,
// empties the frame (its conversation stops asking for new messages) and gives the focus back to
// the button, for a keyboard; a tap or a click leaves the button without the focus, and so without
// a ring. A response that isn't a conversation (the sign-in page, once the session ran out) opens
// as a page of its own.
const PHONE = "(max-width: 719px)";
const WIDTH = 400; // the popup's, in px: 25rem
const TALLEST = 520;
const SHORTEST = 360;
const GAP = 8;

export default class extends Controller {
  static targets = ["sheet", "body", "frame"];

  connect() {
    this.outside = (event) => {
      if (this.sheetTarget.hidden || this.phone) return;
      if (this.sheetTarget.contains(event.target) || event.target.closest?.("[data-action~='message-sheet#open']")) return;
      this.close(event);
    };
    // A drag on the head isn't a click on its link.
    this.afterDrag = (event) => {
      if (!this.justDragged) return;
      this.justDragged = false;
      event.preventDefault();
      event.stopPropagation();
    };
    if (this.hasSheetTarget) {
      document.addEventListener("click", this.outside);
      this.sheetTarget.addEventListener("click", this.afterDrag, true);
    }
  }

  get phone() {
    return matchMedia(PHONE).matches;
  }

  open(event) {
    if (!this.hasSheetTarget) return;
    this.button = event.currentTarget;
    if (event.detail) this.button.blur();
    this.sheetTarget.hidden = false;
    if (this.phone) document.documentElement.classList.add("sheet-open");
    else this.place();
  }

  // The popup under the button, or over it where there is more room above; within the screen's
  // width; as tall as the room allows, between SHORTEST and TALLEST, the page scrolled to show it
  // whole where it doesn't fit. In the page's coordinates, so that it scrolls with the button.
  place() {
    const sheet = this.sheetTarget;
    const button = this.button.getBoundingClientRect();
    const below = innerHeight - button.bottom - 2 * GAP;
    const above = button.top - 2 * GAP;
    const up = below < SHORTEST && above > below;
    const height = Math.max(SHORTEST, Math.min(TALLEST, up ? above : below));
    const parent = (sheet.offsetParent ?? document.body).getBoundingClientRect();
    const left = Math.max(GAP, Math.min(button.left, document.documentElement.clientWidth - WIDTH - GAP));
    const top = up ? button.top - GAP - height : button.bottom + GAP;
    sheet.classList.toggle("opens-up", up);
    sheet.style.setProperty("--height", `${height}px`);
    sheet.style.setProperty("--left", `${left - parent.left}px`);
    sheet.style.setProperty("--top", `${top - parent.top}px`);
    const box = sheet.getBoundingClientRect();
    if (box.bottom > innerHeight - GAP) scrollBy({ top: box.bottom - innerHeight + GAP, behavior: "smooth" });
    else if (box.top < GAP) scrollBy({ top: box.top - GAP, behavior: "smooth" });
  }

  // Where there is a mouse, the box to write in takes the focus; on a phone it waits for a tap,
  // so that the keyboard doesn't cover the conversation as it opens.
  loaded() {
    if (this.sheetTarget.hidden || !matchMedia("(pointer: fine)").matches) return;
    this.frameTarget.querySelector("textarea")?.focus({ preventScroll: true });
  }

  missing(event) {
    event.preventDefault();
    this.close();
    event.detail.visit(event.detail.response);
  }

  // Dragging down, on a phone: by the bar, or by the head.
  grab(event) {
    if (!this.phone || !event.target.closest(".sheet-grabber, .conversation-head")) return;
    this.dragFrom = event.clientY;
    this.dragged = 0;
    this.lastY = event.clientY;
    this.lastAt = event.timeStamp;
    this.speed = 0;
    this.grabbed = event.target;
    this.grabbed.setPointerCapture?.(event.pointerId);
    this.sheetTarget.classList.add("is-dragging");
  }

  drag(event) {
    if (this.dragFrom == null) return;
    this.dragged = Math.max(0, event.clientY - this.dragFrom);
    const elapsed = event.timeStamp - this.lastAt;
    if (elapsed > 0) this.speed = (event.clientY - this.lastY) / elapsed;
    this.lastY = event.clientY;
    this.lastAt = event.timeStamp;
    this.sheetTarget.style.transform = `translateY(${this.dragged}px)`;
  }

  release() {
    if (this.dragFrom == null) return;
    this.dragFrom = null;
    this.justDragged = this.dragged > 6;
    setTimeout(() => (this.justDragged = false), 400);
    const sheet = this.sheetTarget;
    sheet.classList.remove("is-dragging");
    const away = this.dragged > sheet.offsetHeight / 4 || (this.speed > 0.5 && this.dragged > 10);
    if (!away) {
      sheet.style.transform = "";
      return;
    }
    sheet.style.transform = "translateY(100%)";
    const done = () => {
      sheet.removeEventListener("transitionend", done);
      clearTimeout(this.leaving);
      this.close();
      sheet.style.transform = "";
    };
    sheet.addEventListener("transitionend", done);
    this.leaving = setTimeout(done, 250);
  }

  // The focus goes back to the button only for a keyboard (Escape, or "Mbyll" by Enter, a click of
  // no pointer): a tap or a click leaves it, so no ring shows round the button.
  close(event) {
    if (!this.hasSheetTarget || this.sheetTarget.hidden) return;
    this.sheetTarget.hidden = true;
    document.documentElement.classList.remove("sheet-open");
    this.frameTarget.removeAttribute("src");
    this.frameTarget.innerHTML = '<p class="message-sheet-wait">Po hapet biseda…</p>';
    const byKeyboard = event?.type === "keydown" || (event?.type === "click" && event.detail === 0);
    if (byKeyboard) this.button?.focus({ preventScroll: true });
    else this.button?.blur();
  }

  disconnect() {
    document.documentElement.classList.remove("sheet-open");
    document.removeEventListener("click", this.outside);
    if (this.hasSheetTarget) this.sheetTarget.removeEventListener("click", this.afterDrag, true);
  }
}
