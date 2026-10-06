import { Controller } from "@hotwired/stimulus";

// The listing form follows its category: only the kinds of listing the category takes (and no
// choice at all when it takes one), the condition only where new or used means something, the
// category's usual currency until the seller picks one, and the category's own properties (a
// car's make, model and year), which reload in their frame when the category changes. A model
// list shows the chosen make's models only, and "Tjetër" opens a box for a make or model the list
// lacks. "Falas" and "Me marrëveshje" need no amount. The
// server checks all of it again; without JavaScript every option simply shows.
export default class extends Controller {
  static targets = ["category", "type", "typeField", "conditionField", "currency", "priceType", "amountField", "properties"];

  connect() {
    this.categoryChanged({ initial: true });
    this.priceTypeChanged();
    this.narrow();
    this.currencyTarget.addEventListener("change", () => { this.currencyChosen = true; });
    this.element.addEventListener("change", (event) => {
      if (event.target.matches("select[id^='prop_']")) this.narrow();
    });
    this.element.addEventListener("turbo:frame-load", () => this.narrow());
  }

  categoryChanged(event) {
    const option = this.categoryTarget.selectedOptions[0];
    const types = option?.dataset.types?.split(",").filter(Boolean) ?? [];
    for (const typeOption of this.typeTarget.options) {
      typeOption.hidden = typeOption.disabled = types.length > 0 && !types.includes(typeOption.value);
    }
    if (types.length && !types.includes(this.typeTarget.value)) this.typeTarget.value = types[0];
    this.typeFieldTarget.hidden = types.length === 1;
    this.conditionFieldTarget.hidden = Boolean(option?.value) && !("condition" in option.dataset);
    // On the first render the currency and the fields are what the server chose (or sent back).
    if (event?.initial) return;
    if (!this.currencyChosen && option?.dataset.currency) this.currencyTarget.value = option.dataset.currency;
    this.propertiesTarget.src = `/shto/fushat?category=${encodeURIComponent(option?.value ?? "")}`;
  }

  priceTypeChanged() {
    const needsAmount = ["FIXED", "NEGOTIABLE"].includes(this.priceTypeTarget.value);
    this.amountFieldTarget.hidden = !needsAmount;
  }

  // A select that depends on another (the model on the make) offers only the options under the
  // chosen one, and clears a choice that no longer fits.
  narrow() {
    for (const select of this.element.querySelectorAll("select[data-depends-on]")) {
      const parent = this.element.querySelector(`select[name="${select.dataset.dependsOn}"]`);
      const chosen = parent?.value ?? "";
      for (const group of select.querySelectorAll("optgroup")) {
        const fits = !chosen || group.dataset.parent === chosen;
        group.hidden = !fits;
        for (const option of group.children) option.disabled = !fits;
      }
      if (select.selectedOptions[0]?.disabled) select.value = "";
      // A typed make has no models in the list: the model is typed too.
      if (chosen === "__other__") select.value = "__other__";
    }
    // "Tjetër" opens a box to type the option in.
    for (const input of this.element.querySelectorAll("input[data-other-for]")) {
      const select = this.element.querySelector(`select[name="${input.dataset.otherFor}"]`);
      input.hidden = select?.value !== "__other__";
    }
  }
}
