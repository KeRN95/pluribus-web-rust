import { Controller } from "@hotwired/stimulus";

// Switches between the light and dark theme and remembers the choice in localStorage. Until the
// visitor picks one, the site follows the device setting. The inline script in base.html applies
// a saved choice before the first paint. On the bar's button, pressed on the dark theme, and on the
// menu's switch ("Tema e errët", a checkbox), on there: each follows the other (theme:changed).
export default class extends Controller {
  connect() {
    this.systemDark = window.matchMedia("(prefers-color-scheme: dark)");
    this.update = this.update.bind(this);
    this.systemDark.addEventListener("change", this.update);
    addEventListener("theme:changed", this.update);
    this.update();
  }

  disconnect() {
    this.systemDark.removeEventListener("change", this.update);
    removeEventListener("theme:changed", this.update);
  }

  toggle() {
    const theme = this.current === "dark" ? "light" : "dark";
    try {
      localStorage.setItem("theme", theme);
    } catch {
      // Storage can be unavailable (private browsing); the choice then lasts until a full reload.
    }
    const apply = () => {
      document.documentElement.dataset.theme = theme;
      dispatchEvent(new CustomEvent("theme:changed"));
    };
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (document.startViewTransition && !reducedMotion) {
      document.startViewTransition(apply);
    } else {
      apply();
    }
  }

  get current() {
    return document.documentElement.dataset.theme ?? (this.systemDark.matches ? "dark" : "light");
  }

  update() {
    const dark = this.current === "dark";
    if (this.element.type === "checkbox") this.element.checked = dark;
    else this.element.setAttribute("aria-pressed", String(dark));
  }
}
