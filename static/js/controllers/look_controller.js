import { Controller } from "@hotwired/stimulus";

// The look the visitor picks, kept in their browser (localStorage) as the theme is
// (theme_controller.js): the text's size, "small" or "large" (data-text-size on <html>; the normal
// one none), and the accent, "darkred", "terracotta", "blue" or "olive" (data-accent; lime none),
// which the inline script in base.html puts back before each page's first paint
// (web::THEME_SCRIPT). In the menu's "Pamja dhe gjuha" and the bar's "Aa": each shows which is
// picked, and the others follow when one changes (look:changed).
export default class extends Controller {
  static targets = ["size", "accent"];

  connect() {
    this.update = this.update.bind(this);
    addEventListener("look:changed", this.update);
    this.update();
  }

  disconnect() {
    removeEventListener("look:changed", this.update);
  }

  size({ params: { size } }) {
    this.keep("textSize", size === "normal" ? null : size);
  }

  accent(event) {
    this.keep("accent", event.target.value === "lime" ? null : event.target.value);
  }

  keep(key, value) {
    try {
      if (value) localStorage.setItem(key, value);
      else localStorage.removeItem(key);
    } catch {
      // Storage can be unavailable (private browsing); the choice then lasts until a full reload.
    }
    const root = document.documentElement;
    if (value) root.dataset[key] = value;
    else delete root.dataset[key];
    dispatchEvent(new CustomEvent("look:changed"));
  }

  update() {
    const { textSize = "normal", accent = "lime" } = document.documentElement.dataset;
    for (const button of this.sizeTargets) button.setAttribute("aria-pressed", String(button.dataset.lookSizeParam === textSize));
    for (const swatch of this.accentTargets) swatch.checked = swatch.value === accent;
  }
}
