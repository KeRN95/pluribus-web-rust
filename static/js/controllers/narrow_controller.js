import { Controller } from "@hotwired/stimulus";

// A select that depends on another, a car's model on its make, offers only the options under the
// one chosen, as soon as it is chosen, and drops a choice that no longer fits: in the posting form
// and in the filters, where on phones nothing applies until the sheet's button. In the posting
// form, "Tjetër" opens a box to type a make or model the list lacks. Without JavaScript every
// option simply shows (in the filters, those the server left visible).
export default class extends Controller {
  connect() {
    this.narrow();
    this.changed = (event) => {
      if (event.target.matches("select")) this.narrow();
    };
    // The posting form's properties come in a frame, again with each category.
    this.loaded = () => this.narrow();
    this.element.addEventListener("change", this.changed);
    this.element.addEventListener("turbo:frame-load", this.loaded);
  }

  disconnect() {
    this.element.removeEventListener("change", this.changed);
    this.element.removeEventListener("turbo:frame-load", this.loaded);
  }

  narrow() {
    for (const select of this.element.querySelectorAll("select[data-depends-on]")) {
      const parent = this.element.querySelector(`select[name="${select.dataset.dependsOn}"]`);
      if (!parent) continue;
      for (const group of select.querySelectorAll("optgroup")) {
        const fits = !parent.value || group.dataset.parent === parent.value;
        group.hidden = !fits;
        for (const option of group.children) option.disabled = !fits;
      }
      if (select.selectedOptions[0]?.disabled) select.value = "";
      // A typed make has no models in the list: the model is typed too.
      if (parent.value === "__other__") select.value = "__other__";
    }
    // "Tjetër" opens a box to type the option in.
    for (const input of this.element.querySelectorAll("input[data-other-for]")) {
      const select = this.element.querySelector(`select[name="${input.dataset.otherFor}"]`);
      input.hidden = select?.value !== "__other__";
    }
  }
}
