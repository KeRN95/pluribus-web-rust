import { Controller } from "@hotwired/stimulus";

// The town a browse page is in, kept in a cookie (qyteti), so that the next category opens in it
// (browse.rs, Params::remembering_town): set as the page shows and as a refresh in place changes
// its town (the page's element, browse.html). All of Albania in the map card's menu
// (map_controller.js), the filters' "Gjithë Shqipëria" (filters_controller.js) and the town's chip
// (application.js) forget it.
const COOKIE = "qyteti";
const YEAR = 365 * 24 * 60 * 60;

export function forgetTown() {
  document.cookie = `${COOKIE}=; path=/; max-age=0; samesite=lax`;
}

export default class extends Controller {
  static values = { town: String };

  townValueChanged() {
    if (this.townValue) document.cookie = `${COOKIE}=${encodeURIComponent(this.townValue)}; path=/; max-age=${YEAR}; samesite=lax`;
  }
}
