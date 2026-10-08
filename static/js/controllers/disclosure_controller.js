import { Controller } from "@hotwired/stimulus";

// A <details> that shows more of the page in its place rather than a menu over it: a category
// page's own pill, which shows the other categories at its level after it. It opens and closes
// without scripts, and no tap outside closes it, as one would a menu; this only keeps it as the
// visitor left it while the page refreshes in place (refresh.js), which would close it otherwise.
export default class extends Controller {
  keep(event) {
    if (event.target === this.element && event.detail.attributeName === "open") event.preventDefault();
  }
}
