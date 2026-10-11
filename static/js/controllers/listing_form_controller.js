import { Controller } from "@hotwired/stimulus";

// The listing form follows its category: only the kinds of listing the category takes (and no
// choice at all when it takes one), the condition only where new or used means something, the
// category's usual currency until the seller picks one, and the category's own properties (a
// car's make, model and year), which reload in their frame when the category changes; a model
// follows its make (narrow_controller.js). "Falas" and "Me marrëveshje" need no amount. A
// property says its zone in the town, where the town has known ones, and the zones follow the
// town (narrow_controller.js). On a new listing the zone its title or description names is picked
// for the seller (guessZone). The server checks all of it again; without JavaScript every option
// simply shows.
export default class extends Controller {
  static targets = [
    "category", "type", "typeField", "conditionField", "currency", "priceType", "amountField", "properties",
    "city", "zone", "zoneField", "title", "description",
  ];
  // guess: a new listing's form, whose zone follows what the seller writes.
  static values = { guess: Boolean };

  connect() {
    this.categoryChanged({ initial: true });
    this.priceTypeChanged();
    this.currencyTarget.addEventListener("change", () => { this.currencyChosen = true; });
    // A field the form filled in itself (guessZone) loses its tint as the seller changes it, and a
    // zone the seller (or the AI) picks stands: the words no longer move it; so does one the form
    // comes back with, sent before.
    this.zonePicked = Boolean(this.zoneTarget.value);
    this.picked = (event) => {
      if (this.guessing) return;
      event.target.closest(".field")?.classList.remove("ai-filled");
      if (event.target === this.zoneTarget) this.zonePicked = true;
    };
    this.element.addEventListener("change", this.picked);
  }

  disconnect() {
    this.element.removeEventListener("change", this.picked);
  }

  // The currency's toggle (.currency-choice, its radios): the one of `value` picked, as the form
  // picks it, not the seller.
  pickCurrency(value) {
    const radio = this.currencyTarget.querySelector(`input[value="${CSS.escape(value)}"]`);
    if (radio) radio.checked = true;
  }

  // "Posto si": the lead names whom buyers reach then, the store's number or the poster's own
  // contact (each choice's data-contact, post.rs).
  posted(event) {
    const contact = event.target.selectedOptions?.[0]?.dataset.contact;
    const lead = document.getElementById("poster-contact");
    if (contact && lead) lead.textContent = contact;
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
    this.zoneShown();
    // On the first render the currency and the fields are what the server chose (or sent back).
    if (event?.initial) return;
    if (!this.currencyChosen && option?.dataset.currency) this.pickCurrency(option.dataset.currency);
    this.propertiesTarget.src = `/shto/fushat?category=${encodeURIComponent(option?.value ?? "")}`;
    this.guessZone();
  }

  // "Zona" from what the seller wrote, on a new listing's form, as the title or the description is
  // left, the town picked or the category changed: the zone of the town picked that they name, or,
  // with no town picked, one they name with its town ("Blloku, Tiranë"), which is picked too (POST
  // /shto/zona, named.rs). Either is tinted as the AI's fields are, and the zone follows the words
  // as they change, until the seller, or the AI, picks one. Not for a category whose listings have
  // no zone, once one is chosen.
  async guessZone() {
    const category = this.categoryTarget.selectedOptions[0];
    if (!this.guessValue || this.guessing || this.zonePicked || (category?.value && !("zoned" in category.dataset))) return;
    const body = new URLSearchParams({
      title: this.titleTarget.value,
      description: this.descriptionTarget.value,
      city: this.cityTarget.value,
    });
    const asked = (this.asked = body.toString());
    const answer = await fetch("/shto/zona", { method: "POST", body, headers: { Accept: "application/json" } })
      .then((response) => (response.ok ? response.json() : null))
      .catch(() => null);
    // Only the latest answer, and none once the seller has picked a zone meanwhile.
    if (!answer || this.asked !== asked || this.zonePicked) return;
    this.guessing = true;
    try {
      if (answer.city && !this.cityTarget.value) {
        this.cityTarget.value = answer.city;
        this.cityTarget.dispatchEvent(new Event("change", { bubbles: true }));
        this.cityTarget.closest(".field")?.classList.add("ai-filled");
      }
      const zone = answer.city && answer.city === this.cityTarget.value ? answer.zone : "";
      if (this.zoneTarget.value !== zone) {
        this.zoneTarget.value = zone;
        this.zoneTarget.dispatchEvent(new Event("change", { bubbles: true }));
      }
      this.zoneTarget.closest(".field")?.classList.toggle("ai-filled", Boolean(zone));
    } finally {
      this.guessing = false;
    }
  }

  // "Zona": for property, in a town with zones of its own.
  zoneShown() {
    const zoned = "zoned" in (this.categoryTarget.selectedOptions[0]?.dataset ?? {});
    const city = this.cityTarget.value;
    const zones = city && this.zoneTarget.querySelector(`optgroup[data-parent="${CSS.escape(city)}"]`);
    this.zoneFieldTarget.hidden = !(zoned && zones);
  }

  priceTypeChanged() {
    const needsAmount = ["FIXED", "NEGOTIABLE"].includes(this.priceTypeTarget.value);
    this.amountFieldTarget.hidden = !needsAmount;
  }
}
