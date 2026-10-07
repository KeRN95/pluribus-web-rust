import { Controller } from "@hotwired/stimulus";

// The photo picker on the listing form. Photos can be picked a few at a time, as phones offer
// them: each pick adds to the ones before (a file input alone would forget them), up to the number
// still allowed, and each new one shows as a small preview with "Hiq" to take it out again. Big
// phone photos are made smaller before the form goes (longest side MAX_SIDE, JPEG), so posting
// doesn't send megabytes over mobile data. The server checks and re-encodes every photo all the
// same (media.rs); without this script the input sends what was picked last, as it is.

const MAX_SIDE = 2000; // px; the server keeps 1600
const QUALITY = 0.85;

export default class extends Controller {
  static targets = ["input", "previews", "status"];
  static values = { max: Number };

  connect() {
    // What goes with the form, made smaller, in the order picked: the first is the cover.
    this.picked = [];
  }

  async chosen() {
    const room = this.maxValue - this.picked.length;
    let files = [...this.inputTarget.files].filter((file) => file.type.startsWith("image/"));
    let note = "";
    if (files.length > room) {
      files = files.slice(0, Math.max(room, 0));
      note = room > 0
        ? `Mund të shtosh edhe ${room} foto: u morën të parat.`
        : "Njoftimi ka sa foto lejohen. Hiq ndonjë për të shtuar të tjera.";
    }
    if (files.length) this.statusTarget.textContent = "Po përgatiten fotot…";
    this.picked.push(...(await Promise.all(files.map(shrink))));
    this.sync();
    this.statusTarget.textContent = note || this.summary();
  }

  remove(event) {
    this.picked.splice(Number(event.currentTarget.dataset.index), 1);
    this.sync();
    this.statusTarget.textContent = this.summary();
  }

  // The input holds every photo picked, so they all go; the previews show them.
  sync() {
    const transfer = new DataTransfer();
    for (const file of this.picked) transfer.items.add(file);
    this.inputTarget.files = transfer.files;
    this.showPreviews();
  }

  summary() {
    return this.picked.length ? `${this.picked.length} foto gati për t'u ngarkuar.` : "";
  }

  showPreviews() {
    for (const img of this.previewsTarget.querySelectorAll("img")) URL.revokeObjectURL(img.src);
    this.previewsTarget.replaceChildren(
      ...this.picked.map((file, index) => {
        const item = document.createElement("li");
        item.className = "photo-item";
        const remove = Object.assign(document.createElement("button"), { type: "button", className: "photo-remove", textContent: "Hiq" });
        remove.dataset.index = index;
        remove.dataset.action = "photos#remove";
        item.append(Object.assign(document.createElement("img"), { src: URL.createObjectURL(file), alt: "" }), remove);
        return item;
      }),
    );
    this.previewsTarget.hidden = this.picked.length === 0;
  }

  disconnect() {
    for (const img of this.previewsTarget.querySelectorAll("img")) URL.revokeObjectURL(img.src);
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
