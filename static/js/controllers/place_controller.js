import { Controller } from "@hotwired/stimulus";
import { visit } from "@hotwired/turbo";
import { locate } from "controllers/locate.js";

// The places under the browse row (browse.html): all of Albania, around the visitor, or a town. The
// town a page is in is kept in a cookie (qyteti), so that the next category opens in it (browse.rs,
// Params::remembering_town); "Gjithë Shqipëria", the town's chip and the filters' "Gjithë
// Shqipëria" forget it (forgetTown, application.js). "Afër meje" asks the browser where the
// visitor is and searches around it, as far as the distance picked (10 km unless another is), in
// place of a town.
const COOKIE = "qyteti";
const YEAR = 365 * 24 * 60 * 60;

export function forgetTown() {
  document.cookie = `${COOKIE}=; path=/; max-age=0; samesite=lax`;
}

export default class extends Controller {
  static targets = ["status"];
  static values = { town: String, nearUrl: String, radius: Number };

  // The page's town, as it shows, and as a refresh in place changes it.
  townValueChanged() {
    if (this.townValue) document.cookie = `${COOKIE}=${encodeURIComponent(this.townValue)}; path=/; max-age=${YEAR}; samesite=lax`;
  }

  async near() {
    this.say("Po kërkoj vendndodhjen…");
    try {
      const { latitude, longitude } = await locate();
      const url = new URL(this.nearUrlValue, location.href);
      url.searchParams.set("lat", latitude.toFixed(5));
      url.searchParams.set("lng", longitude.toFixed(5));
      url.searchParams.set("radius", String(this.radiusValue || 10));
      this.say("");
      visit(url.href);
    } catch (error) {
      this.say(error.message);
    }
  }

  say(text) {
    if (!this.hasStatusTarget) return;
    this.statusTarget.textContent = text;
    this.statusTarget.hidden = !text;
  }
}
