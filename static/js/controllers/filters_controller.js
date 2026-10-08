import { Controller } from "@hotwired/stimulus";
import { refreshFor } from "controllers/refresh.js";
import { locate } from "controllers/locate.js";
import { forgetTown } from "controllers/place_controller.js";

// The filter panel: a card the "Filtrat" button opens over the results and closes ("Mbyll", or the
// button at its foot that shows the results); on a wide screen a column beside the results, always
// there (WIDE, browse.css). Its options apply by themselves, as soon as one changes (a box ticked,
// a choice picked, a typed field left or entered, a slider let go) and as the card closes, with
// whatever was typed but not left: the page refreshes in place for them (apply, refresh.js), the
// card left as the visitor has it (keep), and the results, their count and the chips follow under
// it. The button at its foot says how many results there are now, from the page's count, which
// once the page shows is asked for further where it stopped at 1 000 (counted, numri=1). Its
// Qyteti replaces the map's point with a town, "Afër meje" the town with the visitor's point. Of
// what the browse row opens (the search, the filters, the map, the order) only one is open at a
// time: opening it says so (section:open), and another's opening folds it (foldFor). Being the
// browse page's own, it also folds whatever is open on Escape, a tap on the dim under it (foldAll)
// or a tap on the row beside its buttons (foldFromRow), and fits what opens into the screen (fit).
const WIDE = "(min-width: 1024px)";

export default class extends Controller {
  static targets = ["panel", "form", "toggle", "tools", "scrim", "show", "placeStatus"];

  connect() {
    this.counted();
  }

  disconnect() {
    clearTimeout(this.timer);
  }

  toggle() {
    if (this.isOpen) this.fold();
    else this.open();
  }

  open() {
    this.panelTarget.classList.add("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "true");
    this.dispatch("open", { target: this.toggleTarget, prefix: "section" });
  }

  // Folds it, applying what was typed but not yet left.
  fold() {
    this.apply();
    this.reset();
  }

  // Folds it as it is, without moving the focus or applying, e.g. before Turbo caches the page for
  // the back button.
  reset() {
    this.panelTarget.classList.remove("is-open");
    this.toggleTarget.setAttribute("aria-expanded", "false");
  }

  // The button at the card's foot: the results are there already, following every change; the card
  // folds, and the page goes to them.
  showResults() {
    const open = this.isOpen;
    this.fold();
    if (open) document.getElementById("results-heading")?.scrollIntoView({ block: "start" });
  }

  // Another card opened.
  foldFor(event) {
    if (event.target !== this.toggleTarget && this.isOpen) this.fold();
  }

  // Escape, or a tap on the dim: whatever the row opened folds, as when another opens
  // (section:open, here from the dim, which opens nothing), and the focus goes back to its button
  // if it was in it. The order's menu folds on its own (dropdown_controller.js).
  foldAll() {
    const toggle = this.toolsTarget.querySelector('.round-button[aria-expanded="true"]');
    if (!toggle) return;
    const inside = document.getElementById(toggle.getAttribute("aria-controls"))?.contains(document.activeElement);
    this.dispatch("open", { target: this.scrimTarget, prefix: "section" });
    if (inside) toggle.focus();
  }

  // A tap on the row itself, beside or between its buttons, rather than on one of them or on what
  // it opened: as on the dim, which the row stands over. (A listener of its own also lets iPhones
  // send the tap at all, and the order's menu closes on it, as on any tap outside it.)
  foldFromRow(event) {
    if (event.target === this.toolsTarget) this.foldAll();
  }

  // What opens floats under the row and takes at most what is left of the screen under it
  // (--top, browse.css), so that its foot shows; it scrolls inside if it needs more.
  fit() {
    this.element.style.setProperty("--top", `${Math.round(this.toolsTarget.getBoundingClientRect().bottom)}px`);
  }

  // A change in the form, a box ticked or a field left: the results follow at once, once the
  // form's other listeners have had it too (narrow_controller.js drops a model that no longer fits
  // its make).
  changed() {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.apply());
  }

  // Enter in a typed field (a year, a distance) applies it, rather than sending the form.
  enter(event) {
    if (!event.target.matches('input[type="text"]')) return;
    event.preventDefault();
    this.apply();
  }

  apply() {
    clearTimeout(this.timer);
    refreshFor(this.formTarget);
  }

  // Qyteti changed: the town replaces the map's point, and "Gjithë Shqipëria" forgets the town kept
  // for the next pages. The form then applies, as for any change (changed).
  placed(event) {
    this.dropPoint();
    if (!event.target.value) forgetTown();
  }

  // "Afër meje": the visitor's point, as far around it as the distance picked, or 10 km, in place
  // of the town.
  async locate() {
    this.say("Po kërkoj vendndodhjen…");
    try {
      const { latitude, longitude } = await locate();
      this.say("");
      const town = this.formTarget.elements.namedItem("city");
      if (town) town.value = "";
      this.setHidden("lat", latitude.toFixed(5));
      this.setHidden("lng", longitude.toFixed(5));
      if (!this.hidden("radius")?.value) this.setHidden("radius", "10");
      this.apply();
    } catch (error) {
      this.say(error.message);
    }
  }

  dropPoint() {
    for (const name of ["lat", "lng", "radius"]) this.hidden(name)?.remove();
  }

  hidden(name) {
    return this.formTarget.querySelector(`input[type="hidden"][name="${name}"]`);
  }

  setHidden(name, value) {
    const input = this.hidden(name)
      ?? this.formTarget.insertAdjacentElement("afterbegin", Object.assign(document.createElement("input"), { type: "hidden", name }));
    input.value = value;
  }

  say(text) {
    if (!this.hasPlaceStatusTarget) return;
    this.placeStatusTarget.textContent = text;
    this.placeStatusTarget.hidden = !text;
  }

  // The page showed, or refreshed in place: the button at the card's foot says what it found, and a
  // count that stopped at 1 000 is asked for further, up to 10 000, for the heading's line and the
  // button both. The latest page's answer only.
  counted() {
    const count = document.getElementById("result-count");
    if (!count) return;
    if (this.hasShowTarget && count.dataset.show) this.showTarget.textContent = count.dataset.show;
    if (!count.hasAttribute("data-capped")) return;
    const url = new URL(location.href);
    url.searchParams.delete("after");
    url.searchParams.set("numri", "1");
    const asked = (this.asked = url.href);
    fetch(url, { headers: { Accept: "application/json" }, cache: "no-store" })
      .then((response) => (response.ok ? response.json() : null))
      .then((answer) => {
        if (!answer || this.asked !== asked || !count.isConnected) return;
        count.textContent = answer.text;
        count.dataset.show = answer.show;
        if (this.hasShowTarget) this.showTarget.textContent = answer.show;
      })
      .catch(() => {});
  }

  // While the page morphs into the refreshed one: an open card stays open, its button saying so,
  // and its form as the visitor has it, while its foot follows the page ("Pastro", once a filter
  // is on); a column beside the results follows the page, form and all (the map's point
  // included), but for the field in use; "Më shumë filtra" stays open or folded as it is; the page
  // keeps its measure (--top); and a field in use elsewhere, the map's distance as it is dragged,
  // keeps its value.
  keep(event) {
    if (event.target === this.formTarget && this.isOpen && !matchMedia(WIDE).matches) event.preventDefault();
  }

  keepAttribute(event) {
    const { attributeName } = event.detail;
    if ((event.target === this.toggleTarget && attributeName === "aria-expanded" && this.isOpen)
      || (event.target === this.panelTarget && attributeName === "class" && this.isOpen)
      || (event.target === this.element && attributeName === "style")
      || (event.target.matches?.("details.more-filters") && attributeName === "open")
      || (event.target === document.activeElement && (attributeName === "value" || attributeName === "checked"))) {
      event.preventDefault();
    }
  }

  // An order picked in Rendit's menu sends the form, and the page reloads in it.
  submit() {
    this.formTarget.requestSubmit();
  }

  get isOpen() {
    return this.panelTarget.classList.contains("is-open");
  }
}
