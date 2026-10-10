import { Controller } from "@hotwired/stimulus";
import { visit } from "@hotwired/turbo";
import { signedIn } from "controllers/signed_in.js";

// The heart over a listing's photo (heart.html). Which listings the visitor keeps comes from the
// server with each page (GET /te-ruajturat/numrat), for every heart on it and those that come later
// (the next page as it scrolls); until it comes, a heart shows what was
// known before. A tap keeps the listing or lets it go at once, every heart for it on the page with
// it, and tells the server (POST /listing/N/ruaj), one tap after the other; should that fail, the
// hearts go back. Someone not signed in goes to sign in, and comes back with the listing kept.
// The same answer says how many kept listings dropped their price unseen, for the top bar's heart
// (the welcome page's, in the row with its search).
// Someone not signed in keeps nothing, and the page doesn't ask (signed_in.js).
// What is known is kept in the browser too (localStorage, STORE), so that a page opened afresh, not
// only one Turbo shows, has its hearts as they were with its photos, before the answer comes; it
// goes once no one is signed in. A browser that won't keep it just waits for the answer.
const STORE = "kept-listings";
let kept = new Set(signedIn() ? stored() : []);
let asking = null;

function stored() {
  try {
    const numbers = JSON.parse(localStorage.getItem(STORE));
    return Array.isArray(numbers) ? numbers : [];
  } catch {
    return [];
  }
}

function store() {
  try {
    if (kept.size) localStorage.setItem(STORE, JSON.stringify([...kept]));
    else localStorage.removeItem(STORE);
  } catch {}
}

function ask() {
  if (!signedIn()) {
    answered({ numbers: [], price_drops: 0 });
    return;
  }
  asking ??= fetch("/te-ruajturat/numrat", { headers: { Accept: "application/json" }, cache: "no-store" })
    .then((response) => (response.ok ? response.json() : null))
    .then((answer) => answer && answered(answer))
    .catch(() => {})
    .finally(() => { asking = null; });
}

function answered(answer) {
  kept = new Set(answer.numbers);
  store();
  for (const heart of document.querySelectorAll(".heart[data-heart-number-value]")) {
    show(heart, kept.has(Number(heart.dataset.heartNumberValue)));
  }
  showDrops(answer.price_drops);
}

// The heart that leads to the listings kept, in the top bar (base.html) or, on the welcome page, in
// the row with its search (home.html), has a dot while listings the visitor keeps dropped their
// price since they last looked at /te-ruajturat, and says how many to screen readers; not on that
// page, which shows them (saved.html) and so has them seen.
function showDrops(count) {
  const news = count > 0 && !document.querySelector(".price-drops");
  const label = news ? `Të ruajturat: ${count} me çmim më të ulët` : "Të ruajturat";
  for (const button of document.querySelectorAll(".saved-shortcut, .saved-button")) {
    button.classList.toggle("has-drops", news);
    button.setAttribute("aria-label", label);
    button.title = label;
  }
}

// Each page asks again: the visitor may have signed in or out, or kept listings elsewhere.
document.addEventListener("turbo:load", ask);

function show(heart, on) {
  heart.querySelector(".heart-button")?.setAttribute("aria-pressed", on ? "true" : "false");
  const field = heart.querySelector('input[name="ruaj"]');
  if (field) field.value = on ? "0" : "1";
}

export default class extends Controller {
  static values = { number: Number };

  connect() {
    show(this.element, kept.has(this.numberValue));
  }

  toggle(event) {
    event.preventDefault();
    const number = this.numberValue;
    const keep = !kept.has(number);
    this.mark(number, keep);
    if (keep) this.beat();
    const send = () => this.send(number, keep);
    this.sending = (this.sending ?? Promise.resolve()).then(send, send);
  }

  async send(number, keep) {
    try {
      const response = await fetch(this.element.action, {
        method: "POST",
        headers: { Accept: "application/json" },
        body: new URLSearchParams({ ruaj: keep ? "1" : "0" }),
      });
      if (response.ok) return;
      this.mark(number, !keep);
      if (response.status === 401) visit("/llogaria");
    } catch {
      this.mark(number, !keep);
    }
  }

  // Every heart for the listing on the page.
  mark(number, keep) {
    if (keep) kept.add(number);
    else kept.delete(number);
    store();
    for (const heart of document.querySelectorAll(`.heart[data-heart-number-value="${number}"]`)) show(heart, keep);
  }

  beat() {
    const button = this.element.querySelector(".heart-button");
    button.classList.remove("is-beating");
    void button.offsetWidth;
    button.classList.add("is-beating");
    button.addEventListener("animationend", () => button.classList.remove("is-beating"), { once: true });
  }
}
