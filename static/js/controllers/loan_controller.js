import { Controller } from "@hotwired/stimulus";
import { t } from "controllers/i18n.js";

// The loan calculator, in the card "Llogarit kredinë" that opens under a listing's price, or in
// the card under a car's key facts (loan_calculator.html): the price less the down payment, a set
// share of the price (the down-percent value, what the banks ask; LOAN_DOWN_PERCENT in
// listings.rs), paid back in equal monthly payments over the years given at the yearly interest
// given (an annuity, as banks lend), with the interest and what it all comes to. Worked out at the
// start, for the fields' first values, and as they change; nothing goes to the server.

const NBSP = " ";

export default class extends Controller {
  static targets = ["years", "rate", "down", "loan", "interest", "total", "monthly", "message"];
  static values = { price: Number, currency: String, downPercent: Number };

  connect() {
    this.calculate();
  }

  calculate() {
    const down = (this.priceValue * this.downPercentValue) / 100;
    const loan = this.priceValue - down;
    this.downTarget.textContent = this.money(down);
    const years = number(this.yearsTarget.value);
    const rate = number(this.rateTarget.value);
    const months = Math.round(years * 12);
    if (!(loan > 0) || !(months >= 1 && months <= 600) || !(rate >= 0 && rate <= 100)) {
      for (const sum of [this.loanTarget, this.interestTarget, this.totalTarget, this.monthlyTarget]) {
        sum.textContent = "–";
      }
      this.messageTarget.textContent = t("Shkruaj vitet (1–50) dhe interesin.");
      this.messageTarget.hidden = false;
      return;
    }
    const monthlyRate = rate / 100 / 12;
    const monthly = monthlyRate === 0
      ? loan / months
      : (loan * monthlyRate) / (1 - (1 + monthlyRate) ** -months);
    const total = monthly * months;
    this.loanTarget.textContent = this.money(loan);
    this.interestTarget.textContent = this.money(total - loan);
    this.totalTarget.textContent = this.money(total);
    this.monthlyTarget.textContent = this.money(monthly);
    this.messageTarget.hidden = true;
  }

  // "1 234 €", whole amounts, like the prices on the site.
  money(amount) {
    const grouped = String(Math.round(amount)).replace(/\B(?=(\d{3})+(?!\d))/g, NBSP);
    return `${grouped}${NBSP}${this.currencyValue}`;
  }
}

// "5,5", "5.5" or "20 000" as a number; nothing for an empty or unreadable field.
function number(text) {
  const cleaned = text.replace(/[\s ]/g, "").replace(",", ".");
  if (cleaned === "") return undefined;
  const value = Number(cleaned);
  return Number.isFinite(value) ? value : undefined;
}
