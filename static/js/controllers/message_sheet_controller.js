import { Controller } from "@hotwired/stimulus";

// A listing's message sheet (listing.html). A message button, signed in, loads the conversation
// into the sheet's frame (data-turbo-frame) and opens it. On a phone it slides up from the screen's
// bottom to its middle (messages.css), the page under it held still; its bar at the top, or its
// head, dragged down past a quarter of its height or flicked down closes it, sliding the rest of
// the way, and let go sooner it goes back up. From 720 px up it is a popup card from the button
// instead: under it, or over it where the screen has more room there, as tall as that room allows,
// the page brought along to show it whole; a click outside it closes it, as a menu's would, and
// so does the button that opened it, pressed again.
// The box to write in takes the focus once the conversation is in, where there is a mouse; on a
// phone the keyboard waits for a tap in it. The keyboard would push the whole screen up to show
// the box, the page and all: instead the sheet rises in the same moment as the box takes the focus
// (raise), before the keyboard comes, onto where the keyboard will be: as high as the keyboard was
// the last time it was up on this screen (KEYBOARD), or a little higher than an iPhone's comes
// until it has been; so the box stands clear of the keyboard and the browser has nothing to push.
// Once the keyboard is up (visualViewport, follow) the sheet eases the rest of the way onto it,
// its foot at the keyboard's top and as tall as it was, or as the room allows, the page under it
// unmoved; and it eases back to the lower half as the focus leaves the box (lower). "Mbyll",
// Escape, or Turbo caching the page closes it, empties the frame (its conversation stops asking
// for new messages) and gives the focus back to the button, for a keyboard; a tap or a click
// leaves the button without the focus, and so without a ring. A response that isn't a conversation
// (the sign-in page, once the session ran out) opens as a page of its own.
const PHONE = "(max-width: 719px)";
const WIDTH = 400; // the popup's, in px: 25rem
const TALLEST = 520;
const SHORTEST = 360;
const GAP = 8;
// The keyboard's height, in localStorage by the screen's width (its way round), from the last time
// it was up; until then, a little over what an iPhone's comes to with its row of suggestions
// (between 42 and 46 per cent of the screen), so that the sheet stands clear whatever the phone.
const KEYBOARD = "keyboard";
const KEYBOARD_GUESS = 0.48;
// The screen's visible part shorter than the screen by less than this is the browser's toolbar
// coming or going, not the keyboard.
const KEYBOARD_LEAST = 100;

export default class extends Controller {
  static targets = ["sheet", "body", "frame"];

  connect() {
    this.outside = (event) => {
      if (this.sheetTarget.hidden || this.phone) return;
      if (this.sheetTarget.contains(event.target) || event.target.closest?.("[data-action~='message-sheet#open']")) return;
      this.close(event);
    };
    this.fit = () => this.follow();
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

  // The button that opened the sheet closes it again, rather than loading the conversation anew.
  open(event) {
    if (!this.hasSheetTarget) return;
    if (!this.sheetTarget.hidden && event.currentTarget === this.button) {
      event.preventDefault();
      this.close(event);
      return;
    }
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

  // The box taking the focus, on a phone: the sheet up onto where the keyboard will be, as tall as
  // it was, in the same moment, before the keyboard comes up (the focus comes first), so the box
  // stands above the keyboard and the browser needn't push the screen up to show it. Laid out there
  // before it is told to ease (is-raised), so that this move is at once and only what follows
  // eases: onto the keyboard where it comes (follow), and down again (lower).
  raise(event) {
    if (!this.phone || !event.target.matches("textarea, input") || this.raised) return;
    const sheet = this.sheetTarget;
    this.raised = true;
    this.keyboardSeen = false;
    this.height = sheet.offsetHeight;
    clearTimeout(this.easing);
    visualViewport?.addEventListener("resize", this.fit);
    visualViewport?.addEventListener("scroll", this.fit);
    this.put(Math.max(GAP, innerHeight - this.keyboard - this.height), this.height);
    sheet.offsetHeight; // the layout, with the sheet there
    sheet.classList.add("is-raised");
  }

  // The keyboard's height on this screen, the last time it was up (KEYBOARD), or the guess.
  get keyboard() {
    let kept;
    try {
      kept = Number(localStorage.getItem(`${KEYBOARD}:${innerWidth}`));
    } catch {}
    return kept || Math.round(innerHeight * KEYBOARD_GUESS);
  }

  remember(keyboard) {
    try {
      localStorage.setItem(`${KEYBOARD}:${innerWidth}`, String(keyboard));
    } catch {}
  }

  // The sheet onto the keyboard, once the screen's visible part has shrunk by it: its foot at the
  // visible part's bottom, as tall as it was or as the room above allows, and the keyboard's
  // height remembered for the next rise. The visible part whole again while the box keeps the
  // focus is the keyboard put away by the phone (Android's back button): the sheet goes down and
  // the box lets the focus go, so that the next tap brings both up again. A change too small to
  // be the keyboard's, the browser's toolbar, moves nothing.
  follow() {
    if (!this.raised) return;
    const view = visualViewport;
    const keyboard = innerHeight - view.height;
    if (keyboard < KEYBOARD_LEAST) {
      if (this.keyboardSeen) {
        this.lower();
        document.activeElement?.blur();
      }
      return;
    }
    this.keyboardSeen = true;
    this.remember(keyboard);
    const height = Math.min(this.height, view.height - GAP);
    this.put(Math.max(view.offsetTop + GAP, view.offsetTop + view.height - height), height);
  }

  // The sheet at `top`, `height` tall; its messages stay at their end where they were.
  put(top, height) {
    const sheet = this.sheetTarget;
    const body = this.bodyTarget;
    const atEnd = body.scrollTop + body.clientHeight >= body.scrollHeight - 120;
    sheet.style.top = `${top}px`;
    sheet.style.height = `${height}px`;
    sheet.style.bottom = "auto";
    if (atEnd) body.scrollTop = body.scrollHeight;
  }

  // The focus leaving the box, and so the keyboard going: the sheet eased back to the screen's
  // lower half (messages.css), and then plain again, so that the next rise is at once. Not for
  // the focus moving within the sheet, to "Mbyll" by a tab.
  lower(event) {
    if (!this.raised || (event?.relatedTarget && this.sheetTarget.contains(event.relatedTarget))) return;
    this.raised = false;
    visualViewport?.removeEventListener("resize", this.fit);
    visualViewport?.removeEventListener("scroll", this.fit);
    const sheet = this.sheetTarget;
    sheet.style.top = sheet.style.height = sheet.style.bottom = "";
    clearTimeout(this.easing);
    this.easing = setTimeout(() => sheet.classList.remove("is-raised"), 250);
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
    // Moved once a frame, at the screen's own rate (120 times a second where it refreshes so),
    // however many moves the finger sends in between.
    this.frame ??= requestAnimationFrame(() => {
      this.frame = null;
      if (this.dragFrom != null) this.sheetTarget.style.transform = `translateY(${this.dragged}px)`;
    });
  }

  release() {
    if (this.dragFrom == null) return;
    this.dragFrom = null;
    cancelAnimationFrame(this.frame);
    this.frame = null;
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
    this.lower();
    this.sheetTarget.hidden = true;
    document.documentElement.classList.remove("sheet-open");
    this.frameTarget.removeAttribute("src");
    this.frameTarget.innerHTML = '<p class="message-sheet-wait">Po hapet biseda…</p>';
    const byKeyboard = event?.type === "keydown" || (event?.type === "click" && event.detail === 0);
    if (byKeyboard) this.button?.focus({ preventScroll: true });
    else this.button?.blur();
  }

  disconnect() {
    if (this.hasSheetTarget) this.lower();
    clearTimeout(this.easing);
    document.documentElement.classList.remove("sheet-open");
    document.removeEventListener("click", this.outside);
    if (this.hasSheetTarget) this.sheetTarget.removeEventListener("click", this.afterDrag, true);
  }
}
