import { Controller } from "@hotwired/stimulus";
import { shrink } from "controllers/photos_controller.js";

// "Plotëso pjesën tjetër me AI" on the posting form: the title, the description and the first
// photos go to the server (web/ai_fill.rs), which has the AI read them and checks its answer
// against the form's own lists. The form then fills in as a seller fills it: the category first,
// then, once its own fields have loaded (listing_form_controller.js), those in their order, a make
// before its model (narrow_controller.js), and the kind, the condition, the price and the town,
// each picker following its select. A field the seller set themselves keeps what they chose, and a
// filled one is tinted until they change it. Nothing is published: the seller checks it all and
// publishes it. Without JavaScript there is no AI to ask.

const PHOTOS = 4; // the server looks at no more
const PHOTO_SIDE = 1024; // and makes them this small anyway: smaller ones go up quicker
const FAILED = "AI nuk u përgjigj tani. Plotëso vetë fushat e tjera, ose provo pak më vonë.";

export default class extends Controller {
  static targets = ["button", "status"];
  static outlets = ["photos"];
  // listing: on an edit, the listing's number, whose own photos the AI sees too.
  static values = { listing: Number };

  connect() {
    // The fields the seller changed themselves, in the pickers or by typing: a fill leaves them
    // as they are. The changes a fill makes itself say so as they happen (emit).
    this.byHand = new Set();
    this.noted = (event) => {
      if (this.filling || !event.target.name) return;
      this.byHand.add(event.target.name);
      event.target.closest(".field")?.classList.remove("ai-filled");
    };
    this.element.addEventListener("input", this.noted);
    this.element.addEventListener("change", this.noted);
    // An edit's form comes filled in: what it holds stands, and a fill fills what is missing.
    if (this.listingValue) {
      for (const element of this.element.elements) {
        if (element.name && (element.type === "checkbox" ? element.checked : element.value)) this.byHand.add(element.name);
      }
    }
  }

  disconnect() {
    this.element.removeEventListener("input", this.noted);
    this.element.removeEventListener("change", this.noted);
  }

  async fill() {
    const form = this.element;
    const title = form.elements.title.value.trim();
    const description = form.elements.description.value.trim();
    if (title.length < 5 || description.length < 10) {
      this.say("Shkruaj së pari titullin dhe përshkrimin: AI plotëson pjesën tjetër prej tyre.");
      (title.length < 5 ? form.elements.title : form.elements.description).focus();
      return;
    }
    this.busy(true);
    this.say("AI po e lexon njoftimin…");
    try {
      const body = new FormData();
      body.append("title", title);
      body.append("description", description);
      // A category the seller chose themselves stands.
      if (this.byHand.has("category") && form.elements.category.value) body.append("category", form.elements.category.value);
      if (this.listingValue) body.append("listing", String(this.listingValue));
      const photos = this.hasPhotosOutlet ? this.photosOutlet.picked.slice(0, PHOTOS) : [];
      for (const photo of await Promise.all(photos.map((file) => shrink(file, PHOTO_SIDE)))) body.append("photos", photo);
      const response = await fetch("/shto/ploteso", { method: "POST", body, headers: { Accept: "application/json" } });
      const answer = await response.json().catch(() => ({}));
      if (!response.ok) {
        this.say(answer.message ?? FAILED);
        return;
      }
      const filled = await this.apply(answer);
      this.say(filled
        ? "AI plotësoi fushat me ngjyrë. Kontrolloji dhe ndrysho çfarë s'është e saktë."
        : "AI nuk gjeti gjë për të plotësuar. Plotësoji vetë fushat.");
    } catch {
      this.say(FAILED);
    } finally {
      this.busy(false);
    }
  }

  // The answer into the form, in the order a seller fills it in; how many fields it filled.
  async apply({ fields = {}, properties = {} }) {
    const form = this.element;
    let filled = 0;
    const fill = (element, value) => {
      if (this.set(element, value)) filled += 1;
    };
    // The category first: its own fields load, and then they fill in.
    const category = form.elements.category;
    if (fields.category && !this.byHand.has("category")) {
      if (category.value === fields.category) {
        this.mark(category);
        filled += 1;
      } else {
        const loaded = frameLoad(form.querySelector("#listing-properties"));
        fill(category, fields.category);
        await loaded;
      }
    }
    for (const name of ["listing_type", "condition", "price_type", "price", "currency", "city", "zone"]) {
      if (name in fields && !this.byHand.has(name)) fill(form.elements.namedItem(name), fields[name]);
    }
    // The category's own fields in their order, so that a make comes before its model.
    const names = new Set([...form.querySelectorAll("[name^='prop_']")].map((element) => element.name));
    for (const name of names) {
      const values = properties[name];
      if (!values || this.byHand.has(name)) continue;
      const boxes = [...form.querySelectorAll(`input[type="checkbox"][name="${CSS.escape(name)}"]`)];
      if (boxes.length) {
        if (this.tick(boxes, values)) filled += 1;
      } else {
        fill(form.elements.namedItem(name), values[0]);
      }
    }
    return filled;
  }

  // Sets a field as the seller would, with the events that follow (its picker, the lists that
  // depend on it), and tints it; false when it has no such choice.
  set(element, value) {
    if (!element || element instanceof RadioNodeList) return false;
    element.value = value;
    if (element.value !== String(value)) return false;
    this.emit(element, element.tagName === "SELECT" ? "change" : "input");
    this.mark(element);
    return true;
  }

  // A list of options to tick (a car's equipment): ticked as the answer says.
  tick(boxes, values) {
    for (const box of boxes) {
      const checked = values.includes(box.value);
      if (box.checked !== checked) {
        box.checked = checked;
        this.emit(box, "change");
      }
    }
    this.mark(boxes[0]);
    return boxes.some((box) => box.checked);
  }

  // An event for a change the fill made, which noted() then knows isn't the seller's.
  emit(element, type) {
    this.filling = true;
    try {
      element.dispatchEvent(new Event(type, { bubbles: true }));
    } finally {
      this.filling = false;
    }
  }

  mark(element) {
    element.closest(".field")?.classList.add("ai-filled");
  }

  busy(on) {
    this.buttonTarget.disabled = on;
    this.buttonTarget.setAttribute("aria-busy", String(on));
  }

  say(text) {
    this.statusTarget.textContent = text;
  }
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
