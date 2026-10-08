import { Controller } from "@hotwired/stimulus";

// A listing's message sheet (listing.html). A message button, signed in, loads the conversation
// into the sheet's frame (data-turbo-frame) and opens it: the page scrolls to bring the button near
// the top, under the header, and the sheet takes the rest of the screen below it (--top), the page
// under it held still. The box to write in takes the focus once the conversation is in. "Mbyll",
// Escape, or Turbo caching the page closes it, empties the frame (its conversation stops asking for
// new messages) and gives the focus back to the button. A response that isn't a conversation (the
// sign-in page, once the session ran out) opens as a page of its own.
export default class extends Controller {
  static targets = ["sheet", "body", "frame"];

  open(event) {
    if (!this.hasSheetTarget) return;
    this.button = event.currentTarget;
    const header = document.querySelector(".site-header")?.getBoundingClientRect().bottom ?? 0;
    scrollBy({ top: this.button.getBoundingClientRect().top - header - 12, behavior: "instant" });
    // Where the page couldn't scroll that far, the sheet still leaves the screen's lower half and more.
    const top = Math.min(this.button.getBoundingClientRect().bottom + 10, innerHeight * 0.45);
    this.sheetTarget.style.setProperty("--top", `${Math.max(top, header)}px`);
    this.sheetTarget.hidden = false;
    document.documentElement.classList.add("sheet-open");
  }

  loaded() {
    if (this.sheetTarget.hidden) return;
    this.frameTarget.querySelector("textarea")?.focus({ preventScroll: true });
  }

  missing(event) {
    event.preventDefault();
    this.close();
    event.detail.visit(event.detail.response);
  }

  close() {
    if (!this.hasSheetTarget || this.sheetTarget.hidden) return;
    this.sheetTarget.hidden = true;
    document.documentElement.classList.remove("sheet-open");
    this.frameTarget.removeAttribute("src");
    this.frameTarget.innerHTML = '<p class="message-sheet-wait">Po hapet biseda…</p>';
    this.button?.focus({ preventScroll: true });
  }

  disconnect() {
    document.documentElement.classList.remove("sheet-open");
  }
}
