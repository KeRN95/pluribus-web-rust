import { Controller } from "@hotwired/stimulus";

// A store's logo or cover on its form (store_form.html), as the canvas draws them: its frame, round
// or wide, with the picture there now or a dashed one with a camera, and "Zgjidh foton" beside it,
// a label that opens the file field's picker. The field stays the real one, for the form and
// screen readers, out of sight once this runs; without it the field shows as it is and the label
// goes (posting.css, .picture-pick). A picture chosen shows in the frame at once.
export default class extends Controller {
  static targets = ["input", "frame"];

  connect() {
    this.element.classList.add("has-picker");
  }

  disconnect() {
    this.element.classList.remove("has-picker");
    this.forget();
  }

  // The picture chosen, in the frame (an image made for it where there was none); none chosen, the
  // one there before, or the empty frame again.
  show() {
    const frame = this.frameTarget;
    let image = frame.querySelector("img");
    if (image && image.dataset.before === undefined) image.dataset.before = image.getAttribute("src") ?? "";
    const file = this.inputTarget.files?.[0];
    this.forget();
    if (file && file.type.startsWith("image/")) {
      if (!image) {
        image = document.createElement("img");
        image.alt = "";
        image.dataset.before = "";
        frame.prepend(image);
      }
      this.url = URL.createObjectURL(file);
      image.src = this.url;
    } else if (image?.dataset.before) {
      image.src = image.dataset.before;
    } else {
      image?.remove();
    }
  }

  forget() {
    if (this.url) URL.revokeObjectURL(this.url);
    this.url = null;
  }
}
