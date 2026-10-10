import { Controller } from "@hotwired/stimulus";
import { t, tf } from "controllers/i18n.js";

// A new listing's form keeps what is typed, in this browser (localStorage, KEY), a moment after
// each change: the title, the description, the category and its own fields, the price, the place,
// and for someone not signed in the contact; never the photos, which a page can't keep, nor the
// form's own hidden fields. Back on an empty form within DAYS, it offers what was kept ("Vazhdo",
// which fills it in as the seller would, the category first and its fields once they have loaded,
// a make before its model; or "Fshije"). A listing that goes up, or waits for its code, takes the
// draft with it (turbo:submit-end, a success); a form sent back with an error keeps it. Without
// storage (a private window that refuses it), nothing is kept, and the form is as it was.

const KEY = "pluribus-draft";
const DAYS = 30;
// The months as the site writes them (format.rs): browsers' own Albanian dates vary, or are
// English. And on an English page, in English.
const MONTHS = ["janar", "shkurt", "mars", "prill", "maj", "qershor", "korrik", "gusht", "shtator", "tetor", "nëntor", "dhjetor"];
const MONTHS_EN = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
// Fields of the form's own, or the photos': none of a draft.
const SKIP = new Set(["started", "website", "photos", "photo_order", "photos_kept", "remove_photo"]);

export default class extends Controller {
  static targets = ["note", "title", "when"];

  connect() {
    this.save = this.save.bind(this);
    this.update = () => {
      clearTimeout(this.waiting);
      this.waiting = setTimeout(this.save, 400);
    };
    this.sent = (event) => {
      if (event.detail?.success) forget();
    };
    this.element.addEventListener("input", this.update);
    this.element.addEventListener("change", this.update);
    this.element.addEventListener("turbo:submit-end", this.sent);
    const draft = read();
    const empty = !this.element.elements.title?.value.trim() && !this.element.elements.description?.value.trim();
    if (draft && empty) this.offer(draft);
  }

  disconnect() {
    clearTimeout(this.waiting);
    this.element.removeEventListener("input", this.update);
    this.element.removeEventListener("change", this.update);
    this.element.removeEventListener("turbo:submit-end", this.sent);
  }

  // What the form holds now, by each field's name: a list for a group of boxes ticked.
  fields() {
    const fields = {};
    for (const element of this.element.elements) {
      const name = element.name;
      if (!name || SKIP.has(name) || element.disabled || element.type === "file" || element.type === "submit" || element.type === "button") continue;
      if (element.type === "checkbox") {
        if (element.checked) (fields[name] ??= []).push(element.value);
      } else {
        fields[name] = element.value;
      }
    }
    return fields;
  }

  save() {
    if (this.restoring) return;
    const fields = this.fields();
    if (!(fields.title ?? "").trim() && !(fields.description ?? "").trim()) return;
    try {
      localStorage.setItem(KEY, JSON.stringify({ saved: Date.now(), fields }));
    } catch {
      // No storage: nothing is kept.
    }
  }

  offer(draft) {
    this.draft = draft;
    this.titleTarget.textContent = draft.fields.title?.trim() ? tf("„{}“", draft.fields.title.trim()) : t("pa titull");
    this.whenTarget.textContent = when(draft.saved);
    this.noteTarget.hidden = false;
  }

  async restore() {
    const fields = this.draft?.fields;
    if (!fields) return;
    this.restoring = true;
    this.noteTarget.hidden = true;
    const form = this.element;
    // The category first: its own fields load, and then they fill in.
    if (fields.category && form.elements.category?.value !== fields.category) {
      const loaded = frameLoad(form.querySelector("#listing-properties"));
      set(form.elements.category, fields.category);
      await loaded;
    }
    for (const name of ["si", "listing_type", "condition", "title", "description", "price_type", "price", "currency", "city", "zone", "contact", "name"]) {
      if (name in fields) set(form.elements.namedItem(name), fields[name]);
    }
    // The category's own fields in the form's order, so that a make comes before its model, and a
    // "Tjetër" before what was typed for it.
    const names = [...new Set([...form.querySelectorAll("[name^='prop_']")].map((element) => element.name))];
    for (const name of names) {
      if (!(name in fields)) continue;
      const boxes = [...form.querySelectorAll(`input[type="checkbox"][name="${CSS.escape(name)}"]`)];
      if (boxes.length) {
        for (const box of boxes) {
          const ticked = [].concat(fields[name]).includes(box.value);
          if (box.checked !== ticked) {
            box.checked = ticked;
            box.dispatchEvent(new Event("change", { bubbles: true }));
          }
        }
      } else {
        set(form.elements.namedItem(name), fields[name]);
      }
    }
    this.restoring = false;
    this.save();
    form.elements.title?.focus();
  }

  discard() {
    forget();
    this.noteTarget.hidden = true;
  }
}

// When it was kept, after "të ruajtur" (saved): "sot në 14:32", "dje në 9:05", "më 10 tetor"; on
// an English page in its own words, "5 minutes ago" within the hour, "today at 14:32", "yesterday
// at 9:05", "on 10 October".
function when(time) {
  const saved = new Date(time);
  const day = (date) => new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const days = Math.round((day(new Date()) - day(saved)) / 86_400_000);
  const clock = `${saved.getHours()}:${String(saved.getMinutes()).padStart(2, "0")}`;
  if (document.documentElement.lang === "en") {
    const minutes = Math.floor((Date.now() - saved.getTime()) / 60_000);
    if (days === 0 && minutes < 1) return "just now";
    if (days === 0 && minutes === 1) return "1 minute ago";
    if (days === 0 && minutes < 60) return `${minutes} minutes ago`;
    if (days === 0) return `today at ${clock}`;
    if (days === 1) return `yesterday at ${clock}`;
    return `on ${saved.getDate()} ${MONTHS_EN[saved.getMonth()]}`;
  }
  if (days === 0) return `sot në ${clock}`;
  if (days === 1) return `dje në ${clock}`;
  return `më ${saved.getDate()} ${MONTHS[saved.getMonth()]}`;
}

function read() {
  try {
    const draft = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (!draft?.fields || Date.now() - draft.saved > DAYS * 86_400_000) return null;
    return draft;
  } catch {
    return null;
  }
}

function forget() {
  try {
    localStorage.removeItem(KEY);
  } catch {
    // No storage: nothing was kept.
  }
}

// Sets a field as the seller would, with the events that follow (its picker, the lists that depend
// on it, the zone that follows the town).
function set(element, value) {
  if (!element || element instanceof RadioNodeList || value === undefined) return;
  element.value = value;
  element.dispatchEvent(new Event(element.tagName === "SELECT" ? "change" : "input", { bubbles: true }));
}

// Resolves once the frame has loaded what it was just sent for, or after a while all the same.
function frameLoad(frame) {
  return new Promise((resolve) => {
    if (!frame) {
      resolve();
      return;
    }
    const done = () => {
      clearTimeout(timer);
      frame.removeEventListener("turbo:frame-load", done);
      resolve();
    };
    const timer = setTimeout(done, 10_000);
    frame.addEventListener("turbo:frame-load", done);
  });
}
