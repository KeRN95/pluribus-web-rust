import { Controller } from "@hotwired/stimulus";

// Switches between the light and dark theme and remembers the choice in localStorage. Until the
// visitor picks one, the site follows the device setting. The inline script in base.html applies
// a saved choice before the first paint.
export default class extends Controller {
  connect() {
    this.systemDark = window.matchMedia("(prefers-color-scheme: dark)");
    this.update = this.update.bind(this);
    this.systemDark.addEventListener("change", this.update);
    this.update();
  }

  disconnect() {
    this.systemDark.removeEventListener("change", this.update);
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
      this.update();
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
    this.element.setAttribute("aria-pressed", String(this.current === "dark"));
  }
}
