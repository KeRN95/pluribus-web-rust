import { Controller } from "@hotwired/stimulus";

// The listing form follows its category: only the kinds of listing the category takes (and no
// choice at all when it takes one), the condition only where new or used means something, the
// category's usual currency until the seller picks one, and the category's own properties (a
// car's make, model and year), which reload in their frame when the category changes; a model
// follows its make (narrow_controller.js). "Falas" and "Me marrëveshje" need no amount. The
// server checks all of it again; without JavaScript every option simply shows.
export default class extends Controller {
  static targets = ["category", "type", "typeField", "conditionField", "currency", "priceType", "amountField", "properties"];

  connect() {
    this.categoryChanged({ initial: true });
    this.priceTypeChanged();
    this.currencyTarget.addEventListener("change", () => { this.currencyChosen = true; });
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
}
