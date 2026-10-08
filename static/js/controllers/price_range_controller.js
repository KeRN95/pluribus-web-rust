import { Controller } from "@hotwired/stimulus";

// The price filter: typed, from and to; one track with two ends, the lowest price and the highest,
// which stop at each other; or one of the quick choices under it, each about a quarter of the
// category's listings (filters.rs, price_choices). Each moves the others. The track's scale is
// logarithmic: STEPS positions over DECADES decades from 10 € (or a hundred times as many Lekë,
// the rate listings' prices are compared at), a step worth more the further right, prices kept to
// two figures. Either end at its edge is no limit. The typed fields carry the prices, the label
// beside the name says them as their chip does (filters.rs, range_label), and the coral stretch
// lies between the ends. As filters.rs.
const STEPS = 1000;
const DECADES = 5;
const BASE = { EUR: 10, ALL: 1000 };
const UNIT = { EUR: "€", ALL: "Lekë" };

export default class extends Controller {
  static targets = ["from", "to", "min", "max", "label", "fill", "currency", "choice"];

  connect() {
    this.paint();
  }

  // A price typed, and left: written plain ("5 000" as 5000), the ends move to it, the label says
  // it; the form applies it, as for any change (filters_controller.js).
  typed() {
    const min = this.read(this.minTarget);
    const max = this.read(this.maxTarget);
    this.minTarget.value = min ?? "";
    this.maxTarget.value = max ?? "";
    this.fromTarget.value = min === null ? 0 : Math.max(1, this.positionOf(min));
    this.toTarget.value = max === null ? STEPS : Math.min(STEPS - 1, this.positionOf(max));
    this.labelTarget.textContent = this.label(min, max);
    this.paint();
  }

  // A quick choice: its prices and its currency, as if typed, and the form applies them; tapped
  // again, it lets go of them.
  pick({ currentTarget, params: { min, max, currency } }) {
    const again = currentTarget.getAttribute("aria-pressed") === "true";
    this.currencyTarget.value = currency;
    this.minTarget.value = again ? "" : min;
    this.maxTarget.value = again ? "" : max;
    this.typed();
    this.minTarget.dispatchEvent(new Event("change", { bubbles: true }));
  }

  // The quick choice the prices are, if they are one, shows pressed.
  mark() {
    for (const choice of this.choiceTargets) {
      const { priceRangeMinParam: min, priceRangeMaxParam: max, priceRangeCurrencyParam: currency } = choice.dataset;
      const pressed = min === this.minTarget.value && max === this.maxTarget.value && currency === this.currency;
      choice.setAttribute("aria-pressed", String(pressed));
    }
  }

  // A typed price as a whole number: "5 000", "5.000" and "5000" alike; null for none.
  read(input) {
    const digits = input.value.split(",")[0].replace(/\D/g, "");
    return digits ? Number(digits) : null;
  }

  // Where a price stands on the track.
  positionOf(price) {
    const position = (Math.log10(Math.max(price, BASE[this.currency]) / BASE[this.currency]) / DECADES) * STEPS;
    return Math.round(Math.min(STEPS, Math.max(0, position)));
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

  // The coral stretch between the ends; the lower end on top once it is in the upper half, so
  // that two ends at the right edge can still be parted.
  paint() {
    const from = Number(this.fromTarget.value);
    const to = Number(this.toTarget.value);
    this.fillTarget.style.left = `${(from / STEPS) * 100}%`;
    this.fillTarget.style.right = `${100 - (to / STEPS) * 100}%`;
    this.fromTarget.classList.toggle("is-top", from > STEPS / 2);
    this.mark();
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
