// Turbo turns links and forms into fetch requests; Stimulus attaches small controllers to HTML.
import { FetchMethod, FetchRequest } from "@hotwired/turbo";
import { Application } from "@hotwired/stimulus";
import AiFillController from "controllers/ai_fill_controller.js";
import CardsController from "controllers/cards_controller.js";
import ConversationController from "controllers/conversation_controller.js";
import CopyController from "controllers/copy_controller.js";
import DragRowController from "controllers/drag_row_controller.js";
import DropdownController from "controllers/dropdown_controller.js";
import FiltersController from "controllers/filters_controller.js";
import GalleryController from "controllers/gallery_controller.js";
import HeartController from "controllers/heart_controller.js";
import InboxController from "controllers/inbox_controller.js";
import ListingFormController from "controllers/listing_form_controller.js";
import LoanController from "controllers/loan_controller.js";
import MapController from "controllers/map_controller.js";
import NarrowController from "controllers/narrow_controller.js";
import NewestController from "controllers/newest_controller.js";
import PanelController from "controllers/panel_controller.js";
import PickerController from "controllers/picker_controller.js";
import PlaceController, { forgetTown } from "controllers/place_controller.js";
import PhotosController from "controllers/photos_controller.js";
import PriceRangeController from "controllers/price_range_controller.js";
import ShareController from "controllers/share_controller.js";
import SlidesController from "controllers/slides_controller.js";
import SuggestController from "controllers/suggest_controller.js";
import ThemeController from "controllers/theme_controller.js";
import { filledIn } from "controllers/refresh.js";

const application = Application.start();
application.register("ai-fill", AiFillController);
application.register("cards", CardsController);
application.register("conversation", ConversationController);
application.register("copy", CopyController);
application.register("drag-row", DragRowController);
application.register("dropdown", DropdownController);
application.register("filters", FiltersController);
application.register("gallery", GalleryController);
application.register("heart", HeartController);
application.register("inbox", InboxController);
application.register("listing-form", ListingFormController);
application.register("loan", LoanController);
application.register("map", MapController);
application.register("narrow", NarrowController);
application.register("newest", NewestController);
application.register("panel", PanelController);
application.register("picker", PickerController);
application.register("place", PlaceController);
application.register("photos", PhotosController);
application.register("price-range", PriceRangeController);
application.register("share", ShareController);
application.register("slides", SlidesController);
application.register("suggest", SuggestController);
application.register("theme", ThemeController);

// A link that drops the town ("Gjithë Shqipëria", the town's chip) forgets it too, before Turbo
// follows it, so the page it opens doesn't open in it again (place_controller.js). Such links are
// never fetched ahead (data-turbo-prefetch), which would have the town still.
document.addEventListener("click", (event) => {
  if (event.target.closest?.("[data-forget-town]")) forgetTown();
}, true);

// A GET form sends every field, filled in or not. Drop the empty ones before Turbo fetches the
// page (filledIn, refresh.js), so filter URLs stay short and shareable; the address bar shows the
// URL that was fetched.
document.addEventListener("turbo:before-fetch-request", (event) => {
  const { url, fetchOptions } = event.detail;
  if (!(event.target instanceof HTMLFormElement) || fetchOptions.method?.toUpperCase() !== "GET") return;
  url.search = filledIn(url.searchParams).toString();
});

// The welcome page's amoeba (thinking.svg) changes shape with SVG animation, which CSS can't stop;
// for visitors who ask for less motion it stands still.
document.addEventListener("turbo:load", () => {
  if (!matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  for (const svg of document.querySelectorAll("svg.thinking")) svg.pauseAnimations();
});

// A tapped card keeps its border (page.css, .card.pressed) while its listing loads; the copy of the
// page Turbo keeps for going back loses it. Only its links open it: a big card's arrows step through
// its photos. iOS only shows :active where a touch listener is.
document.addEventListener("click", (event) => event.target.closest(".card a")?.closest(".card").classList.add("pressed"));
document.addEventListener("turbo:before-cache", () => {
  for (const card of document.querySelectorAll(".card.pressed")) card.classList.remove("pressed");
});

// The copy of the page Turbo keeps for going back has new <img>s. Lazy and decoded aside, as the
// cards' photos are, Safari paints them empty and fills them in a moment later, which shows after
// a swipe back (the button's cross-fade hides it). Photos the page has already loaded go into the
// copy to be shown at once, from the browser's cache; the page itself is leaving.
document.addEventListener("turbo:before-cache", () => {
  for (const img of document.querySelectorAll("img[loading='lazy'], img[decoding='async']")) {
    if (!img.complete || !img.naturalWidth) continue;
    img.removeAttribute("loading");
    img.decoding = "sync";
  }
});
document.addEventListener("touchstart", () => {}, { passive: true });

// Under a mouse, Turbo fetches a link ahead as the pointer rests on it (prefetch on hover), so that
// a click finds the page there; on a phone there is no hovering, and the page is fetched only as
// the finger lifts. So a listing's link starts fetching the moment a finger touches it, and the
// visit the tap then makes takes that response, as it takes Turbo's own (turbo:before-fetch-request,
// fetchRequest); a finger that moves, scrolling, drops it. A page fetched ahead counts no view: it
// counts once it shows (below).
let touched = null;
let touching = false;
// What Turbo tells a request as it goes, which a page fetched ahead has no use for.
const ahead = {
  prepareRequest(request) {
    request.headers["X-Sec-Purpose"] = "prefetch";
  },
  requestStarted() {},
  requestPreventedHandlingResponse() {},
  requestSucceededWithResponse() {},
  requestFailedWithResponse() {},
  requestErrored() {},
  requestFinished() {},
};
const fresh = () => touched && Date.now() - touched.at < 10_000;
document.addEventListener("touchstart", (event) => {
  const link = event.touches.length === 1 && event.target.closest?.('a[href^="/listing/"]:not([target]):not([data-turbo="false"])');
  if (!link) return;
  const url = new URL(link.href);
  if (fresh() && touched.url === url.href) return;
  touched?.request.cancel();
  const request = new FetchRequest(ahead, FetchMethod.get, url, new URLSearchParams(), link);
  request.fetchOptions.priority = "high";
  // Its own turbo:before-fetch-request goes by as it starts.
  touching = true;
  request.perform().catch(() => {});
  touching = false;
  const { clientX: x, clientY: y } = event.touches[0];
  touched = { url: url.href, request, at: Date.now(), x, y };
}, { passive: true });
document.addEventListener("touchmove", (event) => {
  const touch = event.touches[0];
  if (!touched || !touch || Math.hypot(touch.clientX - touched.x, touch.clientY - touched.y) < 10) return;
  touched.request.cancel();
  touched = null;
}, { passive: true });
document.addEventListener("turbo:before-fetch-request", (event) => {
  if (touching || !touched) return;
  const { url, fetchOptions } = event.detail;
  if (fresh() && fetchOptions.method === "GET" && url.href === touched.url) event.detail.fetchRequest = touched.request;
  touched = null;
}, true);

// A listing's page, once Turbo shows it, says it was seen (listings.rs, seen), for the welcome
// page's "Më të klikuarat"; the kept listings' page, that the price drops it shows were seen
// (saved.rs, seen): not when Turbo only fetched them ahead, under the pointer.
document.addEventListener("turbo:load", () => {
  const url = document.querySelector("[data-seen-url]")?.dataset.seenUrl;
  if (url) navigator.sendBeacon(url);
});

// Going back or forward, Turbo shows its copy of the page with the cross-fade base.html asks for
// (view transitions). Where the browser has animated the change itself, as iOS does under a swipe
// from the screen's edge, that would be a second change, from the page left to the page come back
// to, and the page seems to jump: such a visit goes without it, as do those in browsers that can't
// tell. The page shown brings the setting back with its head, as after a refresh (refresh.js).
addEventListener("popstate", (event) => {
  if (event.state?.turbo && (event.hasUAVisualTransition ?? true)) {
    document.head.querySelector('meta[name="view-transition"]')?.remove();
  }
});
