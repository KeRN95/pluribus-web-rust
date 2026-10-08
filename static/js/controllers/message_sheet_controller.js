import { Controller } from "@hotwired/stimulus";

// A listing's message sheet (listing.html). A message button, signed in, loads the conversation
// into the sheet's frame (data-turbo-frame) and opens it: it slides up from the screen's bottom to
// its middle (messages.css), the page under it held still. The box to write in takes the focus
// once the conversation is in, where there is a mouse; on a phone the keyboard waits for a tap in
// it. "Mbyll", Escape, or Turbo caching the page closes it, empties the frame (its conversation
// stops asking for new messages) and gives the focus back to the button, for a keyboard. A
// response that isn't a conversation (the sign-in page, once the session ran out) opens as a page
// of its own. A tap or a click leaves the button without the focus, and so without a ring. Its
// bar at the top, dragged down past a quarter of its height or flicked down, closes it too,
// sliding the rest of the way; let go sooner, it goes back up.
export default class extends Controller {
  static targets = ["sheet", "body", "frame"];

  open(event) {
    if (!this.hasSheetTarget) return;
    this.button = event.currentTarget;
    if (event.detail) this.button.blur();
    this.sheetTarget.hidden = false;
    document.documentElement.classList.add("sheet-open");
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

  grab(event) {
    this.dragFrom = event.clientY;
    this.dragged = 0;
    this.lastY = event.clientY;
    this.lastAt = event.timeStamp;
    this.speed = 0;
    event.currentTarget.setPointerCapture(event.pointerId);
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
  }
}
