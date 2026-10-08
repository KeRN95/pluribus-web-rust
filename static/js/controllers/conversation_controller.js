import { Controller } from "@hotwired/stimulus";
import { renderStreamMessage } from "@hotwired/turbo";

// A conversation (conversation.html). It opens at its latest message, and says what it shows was
// read (POST .../lexuar), then the header's count asks again (inbox_controller.js): only once the
// page shows, not when Turbo fetches it ahead under the pointer. While it is open and in view, it
// asks every few seconds for the messages that came after its last (GET .../te-rejat, a Turbo
// Stream that adds them at the end), follows them down when it was at the end, and reads them too.
// The box grows with what is typed; Enter sends where there is a keyboard and Shift+Enter starts a
// new line, while on a phone Enter is a new line and the button sends; a quick message fills the
// box in.
const EVERY = 8000; // milliseconds

export default class extends Controller {
  static targets = ["thread", "body", "form"];
  static values = { newerUrl: String, readUrl: String };

  connect() {
    this.toEnd();
    // Again once Turbo has put the page at its top, as it does after a visit.
    requestAnimationFrame(() => this.toEnd());
    if (this.hasBodyTarget) this.grow();
    this.observer = new MutationObserver(() => this.came());
    this.observer.observe(this.threadTarget, { childList: true });
    this.read();
    if (this.newerUrlValue) {
      this.timer = setInterval(() => this.poll(), EVERY);
      this.shown = () => document.hidden || this.poll();
      document.addEventListener("visibilitychange", this.shown);
    }
  }

  disconnect() {
    clearInterval(this.timer);
    this.observer.disconnect();
    if (this.shown) document.removeEventListener("visibilitychange", this.shown);
  }

  get lastId() {
    return this.threadTarget.lastElementChild?.dataset.id;
  }

  // What scrolls the messages: the page, or, in a listing's message sheet, the sheet's body.
  get scroller() {
    return this.element.closest("[data-scroll-root]");
  }

  atEnd() {
    const box = this.scroller;
    if (box) return box.scrollTop + box.clientHeight >= box.scrollHeight - 120;
    return innerHeight + scrollY >= document.documentElement.scrollHeight - 120;
  }

  toEnd() {
    const box = this.scroller;
    if (box) box.scrollTop = box.scrollHeight;
    else scrollTo({ top: document.documentElement.scrollHeight });
  }

  async poll() {
    if (document.hidden || this.asking) return;
    this.asking = true;
    try {
      const url = new URL(this.newerUrlValue, location.href);
      if (this.lastId) url.searchParams.set("pas", this.lastId);
      const response = await fetch(url, { headers: { Accept: "text/vnd.turbo-stream.html" }, cache: "no-store" });
      if (response.status !== 200) return;
      this.follow = this.atEnd();
      renderStreamMessage(await response.text());
    } catch {
      // Offline for a moment: the next turn asks again.
    } finally {
      this.asking = false;
    }
  }

  // New messages are in: down to them if the page was at its end, and read.
  came() {
    if (this.follow) this.toEnd();
    this.follow = false;
    this.read();
  }

  // Up to the last message shown, once each; then the header's count asks again.
  async read() {
    const last = this.lastId;
    if (!last || !this.readUrlValue || last === this.readUpTo) return;
    this.readUpTo = last;
    try {
      await fetch(`${this.readUrlValue}?deri=${last}`, { method: "POST", keepalive: true });
      dispatchEvent(new CustomEvent("messages:read"));
    } catch {}
  }

  grow() {
    const box = this.bodyTarget;
    box.style.height = "auto";
    box.style.height = `${box.scrollHeight + 2}px`;
  }

  key(event) {
    if (event.key !== "Enter" || event.shiftKey || event.isComposing) return;
    if (!(event.ctrlKey || event.metaKey) && !matchMedia("(pointer: fine)").matches) return;
    event.preventDefault();
    if (this.bodyTarget.value.trim()) this.formTarget.requestSubmit();
  }

  fill({ params: { text } }) {
    this.bodyTarget.value = text;
    this.grow();
    this.bodyTarget.focus();
  }
}
