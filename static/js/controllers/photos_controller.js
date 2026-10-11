import { Controller } from "@hotwired/stimulus";
import { t, tf } from "controllers/i18n.js";

// The photos on the listing form, one list wherever each came from: those the listing has (on an
// edit) and those picked, a few at a time as phones offer them, or dropped onto the zone from a
// computer's files. The first is the cover, which the cards show, marked "Kopertinë"; each moves
// earlier or later by its arrows, or under a mouse by dragging it onto another's place, and "Hiq"
// takes it out. What goes with the form follows the list: the photos picked, in its order (the
// server keeps them in the order they come), and on an edit the whole order (photo_order:
// "saved:<id>" for one the listing has, "new:<n>" for the n-th sent) and the ones it had that were
// taken out (remove_photo). Big phone photos are made smaller first (longest side MAX_SIDE, JPEG),
// so posting doesn't send megabytes over mobile data. A form the server sends back with an error
// (a 422, which Turbo renders in place) gets its photos back as they were: they wait here, by the
// form's address, across the render (photos_kept tells the server so, which then doesn't ask for
// them again). The server checks and re-encodes every photo all the same (media.rs); without this
// script the form sends what was picked last, as it is, and its saved photos have "Hiq" boxes.

const MAX_SIDE = 1600; // px, as the server keeps them (media.rs)
const QUALITY = 0.85;

// The photos of a form on its way to the server, by its address, for the page that comes back.
const waiting = new Map();

const CHEVRON = (path) =>
  `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${path}"/></svg>`;
const CROSS =
  '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18"/></svg>';

export default class extends Controller {
  static targets = ["input", "previews", "status", "zone", "saved", "order", "kept", "removed"];
  // max: how many photos a listing may have; again: the server sent the form back with photos the
  // browser holds.
  static values = { max: Number, again: Boolean };

  connect() {
    this.key = this.element.closest("form")?.action ?? "";
    const back = waiting.get(this.key);
    waiting.delete(this.key);
    // The photos the listing has, as the page lists them for a form without the script: read, then
    // shown in the one list instead, their "Hiq" boxes out of the form.
    const saved = this.hasSavedTarget
      ? [...this.savedTarget.querySelectorAll("[data-photo-id]")].map((item) => ({
          kind: "saved",
          id: item.dataset.photoId,
          src: item.querySelector("img").src,
        }))
      : [];
    if (this.hasSavedTarget) {
      this.savedTarget.hidden = true;
      for (const box of this.savedTarget.querySelectorAll("input")) box.disabled = true;
    }
    [this.items, this.removed] = this.againValue && back ? [back.items, back.removed] : [saved, []];
    this.keptTarget.value = "1";
    this.urls = [];
    this.sync();
    if (this.againValue && back) this.statusTarget.textContent = this.summary();
  }

  disconnect() {
    waiting.set(this.key, { items: this.items, removed: this.removed });
    for (const url of this.urls) URL.revokeObjectURL(url);
  }

  // The photos picked, in the list's order: what the AI looks at (ai_fill_controller.js).
  get picked() {
    return this.items.filter((item) => item.kind === "new").map((item) => item.file);
  }

  async chosen() {
    await this.add([...this.inputTarget.files]);
  }

  // A file dragged over the field lights its zone, as what is picked is; dropped, it is added as
  // if picked. Anything else dragged (a text, a link, a photo of the list) is left alone here.
  over(event) {
    if (![...(event.dataTransfer?.types ?? [])].includes("Files")) return;
    event.preventDefault();
    this.zoneTarget.classList.add("is-over");
  }

  leave(event) {
    if (event.relatedTarget && this.element.contains(event.relatedTarget)) return;
    this.zoneTarget.classList.remove("is-over");
  }

  async dropped(event) {
    if (!event.dataTransfer?.files?.length) return;
    event.preventDefault();
    this.zoneTarget.classList.remove("is-over");
    await this.add([...event.dataTransfer.files]);
  }

  async add(chosen) {
    const room = this.maxValue - this.items.length;
    let files = chosen.filter((file) => file.type.startsWith("image/"));
    let note = "";
    if (files.length > room) {
      files = files.slice(0, Math.max(room, 0));
      // One photo more has a text of its own, for English ("1 more photo").
      if (room === 1) note = t("Mund të shtosh edhe 1 foto: u morën të parat.");
      else if (room > 1) note = tf("Mund të shtosh edhe {} foto: u morën të parat.", room);
      else note = t("Njoftimi ka sa foto lejohen. Hiq ndonjë për të shtuar të tjera.");
    }
    if (files.length) this.statusTarget.textContent = t("Po përgatiten fotot…");
    const shrunk = await Promise.all(files.map((file) => shrink(file)));
    this.items.push(...shrunk.map((file) => ({ kind: "new", file })));
    this.sync();
    this.statusTarget.textContent = note || this.summary();
  }

  remove(event) {
    const [item] = this.items.splice(Number(event.currentTarget.dataset.index), 1);
    if (item?.kind === "saved") this.removed.push(item.id);
    this.sync();
    this.statusTarget.textContent = this.summary();
  }

  earlier(event) {
    const index = Number(event.currentTarget.dataset.index);
    if (index > 0) this.move(index, index - 1, "earlier");
  }

  later(event) {
    const index = Number(event.currentTarget.dataset.index);
    if (index < this.items.length - 1) this.move(index, index + 1, "later");
  }

  // Under a mouse: a photo of the list dragged onto another's place takes it.
  grab(event) {
    this.dragging = Number(event.currentTarget.dataset.index);
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", "");
    event.currentTarget.classList.add("is-dragging");
  }

  hover(event) {
    if (this.dragging === undefined) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
  }

  place(event) {
    if (this.dragging === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    const from = this.dragging;
    this.dragging = undefined;
    this.move(from, Number(event.currentTarget.dataset.index));
  }

  release(event) {
    this.dragging = undefined;
    event.currentTarget.classList.remove("is-dragging");
  }

  // Moves a photo to another place; the focus follows it when a button moved it, so that the
  // keyboard goes on from there.
  move(from, to, button) {
    const [item] = this.items.splice(from, 1);
    this.items.splice(to, 0, item);
    this.sync();
    this.statusTarget.textContent = to === 0 ? t("Kjo është tani kopertina.") : "";
    if (!button) return;
    const target = this.previewsTarget.querySelector(`[data-index="${to}"][data-action="photos#${button}"]`);
    (target && !target.disabled ? target : this.previewsTarget.querySelector(`[data-index="${to}"]`))?.focus();
  }

  // The input holds every photo picked, in the list's order, so they all go; the order and the
  // photos taken out go beside them; the list shows them.
  sync() {
    const transfer = new DataTransfer();
    const order = [];
    for (const item of this.items) {
      if (item.kind === "new") {
        order.push(`new:${transfer.items.length}`);
        transfer.items.add(item.file);
      } else {
        order.push(`saved:${item.id}`);
      }
    }
    this.inputTarget.files = transfer.files;
    this.orderTarget.value = order.join(",");
    this.removedTarget.replaceChildren(
      ...this.removed.map((id) => Object.assign(document.createElement("input"), { type: "hidden", name: "remove_photo", value: id })),
    );
    this.render();
  }

  summary() {
    const count = this.picked.length;
    if (!count) return "";
    return count === 1 ? t("1 foto gati për t'u ngarkuar.") : tf("{} foto gati për t'u ngarkuar.", count);
  }

  render() {
    for (const url of this.urls) URL.revokeObjectURL(url);
    this.urls = [];
    this.previewsTarget.replaceChildren(...this.items.map((item, index) => this.photo(item, index)));
    this.previewsTarget.hidden = this.items.length === 0;
  }

  // One photo of the list: the picture, "Kopertinë" over the first, the cross that takes it out
  // over its corner, then its arrows under it.
  photo(item, index) {
    const element = document.createElement("li");
    element.className = "photo-item";
    element.draggable = true;
    element.tabIndex = -1;
    element.dataset.index = index;
    element.dataset.action = "dragstart->photos#grab dragover->photos#hover drop->photos#place dragend->photos#release";
    const frame = document.createElement("div");
    frame.className = "photo-frame";
    const image = Object.assign(document.createElement("img"), { alt: "" });
    if (item.kind === "saved") {
      image.src = item.src;
    } else {
      image.src = URL.createObjectURL(item.file);
      this.urls.push(image.src);
    }
    frame.append(image);
    if (index === 0) frame.append(Object.assign(document.createElement("span"), { className: "photo-cover", textContent: t("Kopertinë") }));
    frame.append(this.button(CROSS, null, "remove", index, false));
    const tools = document.createElement("div");
    tools.className = "photo-tools";
    tools.append(
      this.button(CHEVRON("m15 6-6 6 6 6"), t("Më përpara"), "earlier", index, index === 0),
      this.button(CHEVRON("m9 6 6 6-6 6"), t("Më pas"), "later", index, index === this.items.length - 1),
    );
    element.append(frame, tools);
    return element;
  }

  button(content, label, action, index, disabled) {
    const button = Object.assign(document.createElement("button"), { type: "button", disabled });
    button.className = label ? "photo-step" : "photo-remove";
    button.innerHTML = content;
    button.setAttribute("aria-label", label ? tf("{} (foto {})", label, index + 1) : tf("Hiq foton {}", index + 1));
    button.title = label ?? t("Hiq");
    button.dataset.index = index;
    button.dataset.action = `photos#${action}`;
    return button;
  }
}

// A smaller JPEG copy, no longer than `side`, turned the way the camera held it; the original when
// it is small already or the browser can't draw it. The AI gets smaller ones (ai_fill_controller.js).
export async function shrink(file, side = MAX_SIDE) {
  try {
    const bitmap = await createImageBitmap(file, { imageOrientation: "from-image" });
    const scale = Math.min(1, side / Math.max(bitmap.width, bitmap.height));
    if (scale === 1 && file.type === "image/jpeg" && file.size < 1_500_000) return file;
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(bitmap.width * scale);
    canvas.height = Math.round(bitmap.height * scale);
    canvas.getContext("2d").drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    bitmap.close();
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, "image/jpeg", QUALITY));
    if (!blob) return file;
    return new File([blob], file.name.replace(/\.[^.]*$/, "") + ".jpg", { type: "image/jpeg" });
  } catch {
    return file;
  }
}
