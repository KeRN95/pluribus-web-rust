import { Controller } from "@hotwired/stimulus";

// Remembers the chosen city for a year, so the welcome page offers it again (the server reads
// the cookie), and carries it into the links next to the select.
export default class extends Controller {
  static targets = ["select", "link"];

  change() {
    const city = this.selectTarget.value;
    const secure = location.protocol === "https:" ? "; secure" : "";
    document.cookie = city
      ? `city=${encodeURIComponent(city)}; path=/; max-age=31536000; samesite=lax${secure}`
      : `city=; path=/; max-age=0; samesite=lax${secure}`;
    for (const link of this.linkTargets) {
      const url = new URL(link.href);
      if (city) url.searchParams.set("city", city);
      else url.searchParams.delete("city");
      link.href = url.pathname + url.search;
    }
  }
}
