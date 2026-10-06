import { Controller } from "@hotwired/stimulus";

// A <details> menu (the category page's heading) closes again on a tap outside it or on Escape,
// the way menus do; opening and choosing need no JavaScript.
export default class extends Controller {
  connect() {
    this.closeOutside = (event) => {
      if (this.element.open && !this.element.contains(event.target)) this.element.open = false;
    };
    this.closeOnEscape = (event) => {
      if (event.key === "Escape" && this.element.open) {
        this.element.open = false;
        this.element.querySelector("summary")?.focus();
      }
    };
    document.addEventListener("click", this.closeOutside);
    document.addEventListener("keydown", this.closeOnEscape);
  }

  disconnect() {
    document.removeEventListener("click", this.closeOutside);
    document.removeEventListener("keydown", this.closeOnEscape);
  }
}
