import { Controller } from "@hotwired/stimulus";

// The look the visitor picks, kept in their browser (localStorage) as the theme is
// (theme_controller.js): the text's size, "smaller", "small" or "large" (data-text-size on <html>;
// the normal one none), the page's, "90", "80" or "70" (data-page-size, its percent; the whole
// page none), and the accent, "lime", "coral", "orange" or "blue" (data-accent; mint, the
// one there is unless another is picked since 2026-10-11, none), which the inline script in
// base.html puts back before each page's first paint (web::THEME_SCRIPT, which turns an older
// pick, "darkred", "terracotta" or "olive", into its bright one). In the menu's "Pamja dhe gjuha":
// each control shows which is picked, and the others follow when one changes (look:changed).
export default class extends Controller {
  static targets = ["size", "page", "accent"];

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

  page({ params: { page } }) {
    this.keep("pageSize", String(page) === "100" ? null : String(page));
  }

  accent(event) {
    this.keep("accent", event.target.value === "mint" ? null : event.target.value);
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
    const { textSize = "normal", pageSize = "100", accent = "mint" } = document.documentElement.dataset;
    for (const button of this.sizeTargets) button.setAttribute("aria-pressed", String(button.dataset.lookSizeParam === textSize));
    for (const button of this.pageTargets) button.setAttribute("aria-pressed", String(button.dataset.lookPageParam === pageSize));
    for (const swatch of this.accentTargets) swatch.checked = swatch.value === accent;
  }
}
