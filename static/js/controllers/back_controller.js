import { Controller } from "@hotwired/stimulus";

// The round "back" button before a category's pills: to the page before, as the browser's back
// does, when that page was this site's in this tab (Turbo numbers its history entries,
// restorationIndex, from 0 for the first); opened from elsewhere, or without scripts, its link
// leads a level up instead.
export default class extends Controller {
  go(event) {
    if ((history.state?.turbo?.restorationIndex ?? 0) > 0) {
      event.preventDefault();
      history.back();
    }
  }
}
