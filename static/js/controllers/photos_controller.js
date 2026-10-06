import { Controller } from "@hotwired/stimulus";

// The photo picker on the listing form. It shows small previews of the photos chosen, keeps to
// the number still allowed, and makes big phone photos smaller before the form goes (longest side
// MAX_SIDE, JPEG), so posting doesn't send megabytes over mobile data. The server checks and
// re-encodes every photo all the same (media.rs); without this script the originals go as they are.

const MAX_SIDE = 2000; // px; the server keeps 1600
const QUALITY = 0.85;

export default class extends Controller {
  static targets = ["input", "previews", "status"];
  static values = { max: Number };

  async chosen() {
    let files = [...this.inputTarget.files].filter((file) => file.type.startsWith("image/"));
    let note = "";
    if (files.length > this.maxValue) {
      files = files.slice(0, this.maxValue);
      note = this.maxValue
        ? `Mund të shtosh edhe ${this.maxValue} foto: u morën të parat.`
        : "Njoftimi ka sa foto lejohen. Hiq ndonjë për të shtuar të tjera.";
    }
    this.statusTarget.textContent = files.length ? "Po përgatiten fotot…" : note;
    const ready = await Promise.all(files.map(shrink));

    // The input now holds the smaller copies (or the originals, where shrinking didn't work).
    const transfer = new DataTransfer();
    for (const file of ready) transfer.items.add(file);
    this.inputTarget.files = transfer.files;

    this.showPreviews(ready);
    this.statusTarget.textContent = note || (ready.length ? `${ready.length} foto gati për t'u ngarkuar.` : "");
  }

  showPreviews(files) {
    for (const img of this.previewsTarget.querySelectorAll("img")) URL.revokeObjectURL(img.src);
    this.previewsTarget.replaceChildren(
      ...files.map((file) => {
        const item = document.createElement("li");
        item.className = "photo-item";
        item.append(Object.assign(document.createElement("img"), { src: URL.createObjectURL(file), alt: "" }));
        return item;
      }),
    );
    this.previewsTarget.hidden = files.length === 0;
  }

  disconnect() {
    for (const img of this.previewsTarget.querySelectorAll("img")) URL.revokeObjectURL(img.src);
  }
}

// A smaller JPEG copy, turned the way the camera held it; the original when it is small already or
// the browser can't draw it.
async function shrink(file) {
  try {
    const bitmap = await createImageBitmap(file, { imageOrientation: "from-image" });
    const scale = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height));
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
