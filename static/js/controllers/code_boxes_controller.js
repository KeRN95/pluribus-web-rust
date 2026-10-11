import { Controller } from "@hotwired/stimulus";

// The sign-in code's six boxes (sign_in_code.html), as the canvas draws them: the one input the code
// is typed, pasted or filled into (autocomplete="one-time-code", which a phone offers from the
// message), lying over six boxes that show its digits, the next one to fill ringed while the input
// has the focus. The input stays the real field, for the browser, screen readers and the form;
// without the script it shows as it is, the boxes out of sight (posting.css, .code-boxes).
export default class extends Controller {
  static targets = ["input", "box"];

  connect() {
    this.element.classList.add("has-boxes");
    this.show();
  }

  disconnect() {
    this.element.classList.remove("has-boxes");
  }

  // The digits typed into the boxes, one a box (a space typed between them, "482 913", left out),
  // and the ring on the next empty one, or the last when they are full.
  show() {
    const boxes = this.boxTargets;
    const digits = this.inputTarget.value.replace(/\D/g, "").slice(0, boxes.length);
    const focused = document.activeElement === this.inputTarget;
    boxes.forEach((box, at) => {
      box.textContent = digits[at] ?? "";
      box.classList.toggle("is-next", focused && at === Math.min(digits.length, boxes.length - 1));
    });
  }
}
