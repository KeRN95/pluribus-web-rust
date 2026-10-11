import { Controller } from "@hotwired/stimulus";
import { signedIn } from "controllers/signed_in.js";

// The menu (base.html): the bar's "Menyja" opens it, a modal dialog over the page dimmed (a drawer
// at the screen's left, the whole screen on phones: base.css, .menu), and its cross, a tap on the
// dimmed page or Escape closes it, focus going back to the button; Turbo finds it closed when it
// keeps the page for the back button. As it opens it asks how many notifications are new and how
// many conversations have messages unread, for the counts at those rows' ends (GET
// /lajmerimet/numri, /mesazhet/numri), as the bar's buttons ask with each page; someone not signed
// in has none, and it doesn't ask (signed_in.js).
export default class extends Controller {
  static targets = ["button", "dialog", "close", "notifications", "messages"];

  // Focus goes to the cross rather than the logo, the dialog's first link.
  open() {
    if (this.dialogTarget.open) return;
    this.dialogTarget.showModal();
    this.closeTarget.focus();
    this.buttonTarget.setAttribute("aria-expanded", "true");
    this.count();
  }

  close() {
    if (!this.dialogTarget.open) return;
    this.dialogTarget.close();
    this.closed();
  }

  // The dialog closed, whichever way: by close() above, or by Escape (its cancel, then its close
  // event, which a browser may hold back while the page is out of sight).
  closed() {
    this.buttonTarget.setAttribute("aria-expanded", "false");
  }

  // A tap on the dimmed page: the drawer's parts fill the dialog, so a click on the dialog itself
  // landed on its backdrop.
  outside(event) {
    if (event.target === this.dialogTarget) this.close();
  }

  async count() {
    if (!signedIn()) return;
    const ask = async (url) => {
      try {
        const response = await fetch(url, { headers: { Accept: "application/json" }, cache: "no-store" });
        return response.ok ? (await response.json()).unread : 0;
      } catch {
        return 0;
      }
    };
    const [notifications, messages] = await Promise.all([ask("/lajmerimet/numri"), ask("/mesazhet/numri")]);
    this.show(this.notificationsTarget, notifications);
    this.show(this.messagesTarget, messages);
  }

  show(count, unread) {
    count.querySelector("[data-number]").textContent = unread > 9 ? "9+" : String(unread);
    count.hidden = !(unread > 0);
  }
}
