import { Controller } from "@hotwired/stimulus";

// A listing's contact bar on a phone (listing.css, .contact-bar): sticky at the screen's bottom
// while the listing scrolls, it wears a slight shadow over it while it floats over the page
// (.is-stuck), and none once it settles in its place after the description. Whether it floats is
// told by the screen's bottom edge, moved up a pixel (rootMargin): floating, the bar stands on the
// edge and that pixel of it is out, so it no longer shows whole; settled, it stands clear of the
// edge and shows whole. Scrolled past, above the screen, it is settled too (top), whatever shows
// of it.
export default class extends Controller {
  connect() {
    this.watcher = new IntersectionObserver(
      ([entry]) => {
        const floating = entry.intersectionRatio < 1 && entry.boundingClientRect.top > 0;
        this.element.classList.toggle("is-stuck", floating);
      },
      { threshold: [1], rootMargin: "0px 0px -1px 0px" },
    );
    this.watcher.observe(this.element);
  }

  disconnect() {
    this.watcher?.disconnect();
    this.element.classList.remove("is-stuck");
  }
}
