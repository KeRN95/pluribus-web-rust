import { Controller } from "@hotwired/stimulus";

// The price filter: one track with two ends, the lowest price and the highest, which stop at each
// other. The scale is logarithmic, as the map's distance is: STEPS positions over DECADES decades
// from 10 € (or a hundred times as many Lekë, the rate listings' prices are compared at), a step
// worth more the further right, prices kept to two figures. Either end at its edge is no limit.
// The hidden fields carry the prices, the label beside the name says them as their chip does
// (filters.rs, range_label), and the blue stretch lies between the ends. As filters.rs.
const STEPS = 1000;
const DECADES = 5;
const BASE = { EUR: 10, ALL: 1000 };
const UNIT = { EUR: "€", ALL: "Lekë" };

export default class extends Controller {
  static targets = ["from", "to", "min", "max", "label", "fill", "currency"];

  connect() {
    this.paint();
  }

  // An end moved, or the currency changed: the end stops at the other, and the prices follow.
  slide(event) {
    let from = Number(this.fromTarget.value);
    let to = Number(this.toTarget.value);
    if (from > to) {
      if (event?.target === this.fromTarget) this.fromTarget.value = from = to;
      else this.toTarget.value = to = from;
    }
    const min = from > 0 ? this.priceAt(from) : null;
    const max = to < STEPS ? this.priceAt(to) : null;
    this.minTarget.value = min ?? "";
    this.maxTarget.value = max ?? "";
    this.labelTarget.textContent = this.label(min, max);
    this.fromTarget.setAttribute("aria-valuetext", min === null ? "Pa kufi" : this.money(min));
    this.toTarget.setAttribute("aria-valuetext", max === null ? "Pa kufi" : this.money(max));
    this.paint();
  }

  // The blue stretch between the ends; the lower end on top once it is in the upper half, so
  // that two ends at the right edge can still be parted.
  paint() {
    const from = Number(this.fromTarget.value);
    const to = Number(this.toTarget.value);
    this.fillTarget.style.left = `${(from / STEPS) * 100}%`;
    this.fillTarget.style.right = `${100 - (to / STEPS) * 100}%`;
    this.fromTarget.classList.toggle("is-top", from > STEPS / 2);
  }

  priceAt(position) {
    const price = BASE[this.currency] * 10 ** ((DECADES * position) / STEPS);
    const figure = 10 ** (Math.floor(Math.log10(price)) - 1);
    return Math.round(price / figure) * figure;
  }

  label(min, max) {
    const unit = UNIT[this.currency];
    if (min !== null && max !== null) return `${this.number(min)} – ${this.number(max)}\u00a0${unit}`;
    if (min !== null) return `nga ${this.money(min)}`;
    if (max !== null) return `deri ${this.money(max)}`;
    return "Çdo çmim";
  }

  money(price) {
    return `${this.number(price)}\u00a0${UNIT[this.currency]}`;
  }

  number(value) {
    return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, "\u00a0");
  }

  get currency() {
    return this.currencyTarget.value in BASE ? this.currencyTarget.value : "EUR";
  }
}
