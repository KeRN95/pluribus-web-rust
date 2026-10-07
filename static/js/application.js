// Turbo turns links and forms into fetch requests; Stimulus attaches small controllers to HTML.
import "@hotwired/turbo";
import { Application } from "@hotwired/stimulus";
import AiFillController from "controllers/ai_fill_controller.js";
import CardsController from "controllers/cards_controller.js";
import CopyController from "controllers/copy_controller.js";
import DropdownController from "controllers/dropdown_controller.js";
import FiltersController from "controllers/filters_controller.js";
import GalleryController from "controllers/gallery_controller.js";
import HeartController from "controllers/heart_controller.js";
import ListingFormController from "controllers/listing_form_controller.js";
import LoanController from "controllers/loan_controller.js";
import MapController from "controllers/map_controller.js";
import NarrowController from "controllers/narrow_controller.js";
import NewestController from "controllers/newest_controller.js";
import PanelController from "controllers/panel_controller.js";
import PickerController from "controllers/picker_controller.js";
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
application.register("copy", CopyController);
application.register("dropdown", DropdownController);
application.register("filters", FiltersController);
application.register("gallery", GalleryController);
application.register("heart", HeartController);
application.register("listing-form", ListingFormController);
application.register("loan", LoanController);
application.register("map", MapController);
application.register("narrow", NarrowController);
application.register("newest", NewestController);
application.register("panel", PanelController);
application.register("picker", PickerController);
application.register("photos", PhotosController);
application.register("price-range", PriceRangeController);
application.register("share", ShareController);
application.register("slides", SlidesController);
application.register("suggest", SuggestController);
application.register("theme", ThemeController);

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

// A tapped card keeps its border (app.css, .card.pressed) while its listing loads; the copy of the
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
