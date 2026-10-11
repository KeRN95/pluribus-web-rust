import { Controller } from "@hotwired/stimulus";

// The listing form's hint under the price, for a car (Makina) whose model and year are chosen:
// what cars of that model two years either side ask, and, once a price is typed, how it will
// compare on the car's page ("Çmim i mirë", "Rreth mesatares", "Mbi mesataren"), or that it is so
// far under them it is likely wrong (/shto/cmimi, car_prices.rs, which says it all). The model and
// year are in the properties' frame, which reloads with the category: the form's own change and
// input events reach here from wherever they are. Asked again a moment after the typing stops;
// nothing shows for another category, or where too few cars say.
export default class extends Controller {
  static targets = ["hint"];

  connect() {
    this.asked = "";
    this.update = () => {
      clearTimeout(this.waiting);
      this.waiting = setTimeout(() => this.ask(), 250);
    };
    this.element.addEventListener("change", this.update);
    this.element.addEventListener("input", this.update);
    this.element.addEventListener("turbo:frame-load", this.update);
    this.ask();
  }

  disconnect() {
    clearTimeout(this.waiting);
    this.element.removeEventListener("change", this.update);
    this.element.removeEventListener("input", this.update);
    this.element.removeEventListener("turbo:frame-load", this.update);
  }

  // A field's value by its name; for a group of radios (the currency), the one picked.
  value(name) {
    return this.element.elements.namedItem(name)?.value?.trim() ?? "";
  }

  async ask() {
    const model = this.value("prop_vehicle_model");
    const year = this.value("prop_vehicle_year");
    if (this.value("category") !== "makina" || !model || model === "__other__" || !/^\d{4}$/.test(year)) {
      this.show(null);
      return;
    }
    const query = new URLSearchParams({ model, year, currency: this.value("currency"), price: this.value("price") }).toString();
    if (query === this.asked) return;
    this.asked = query;
    try {
      const response = await fetch(`/shto/cmimi?${query}`, { headers: { Accept: "application/json" } });
      if (query !== this.asked) return;
      this.show(response.status === 200 ? await response.json() : null);
    } catch {
      this.show(null);
    }
  }

  show(answer) {
    this.hintTarget.hidden = !answer;
    this.hintTarget.textContent = answer ? [answer.text, answer.verdict].filter(Boolean).join(" ") : "";
  }
}
