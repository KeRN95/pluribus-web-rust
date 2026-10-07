// Turbo turns links and forms into fetch requests; Stimulus attaches small controllers to HTML.
import "@hotwired/turbo";
import { Application } from "@hotwired/stimulus";
import CardsController from "controllers/cards_controller.js";
import CopyController from "controllers/copy_controller.js";
import DropdownController from "controllers/dropdown_controller.js";
import FiltersController from "controllers/filters_controller.js";
import GalleryController from "controllers/gallery_controller.js";
import ListingFormController from "controllers/listing_form_controller.js";
import LoanController from "controllers/loan_controller.js";
import MapController from "controllers/map_controller.js";
import NarrowController from "controllers/narrow_controller.js";
import PickerController from "controllers/picker_controller.js";
import PhotosController from "controllers/photos_controller.js";
import PriceRangeController from "controllers/price_range_controller.js";
import ShareController from "controllers/share_controller.js";
import SlidesController from "controllers/slides_controller.js";
import SuggestController from "controllers/suggest_controller.js";
import ThemeController from "controllers/theme_controller.js";

const application = Application.start();
application.register("cards", CardsController);
application.register("copy", CopyController);
application.register("dropdown", DropdownController);
application.register("filters", FiltersController);
application.register("gallery", GalleryController);
application.register("listing-form", ListingFormController);
application.register("loan", LoanController);
application.register("map", MapController);
application.register("narrow", NarrowController);
application.register("picker", PickerController);
application.register("photos", PhotosController);
application.register("price-range", PriceRangeController);
application.register("share", ShareController);
application.register("slides", SlidesController);
application.register("suggest", SuggestController);
application.register("theme", ThemeController);

// A GET form sends every field, filled in or not. Drop the empty ones before Turbo fetches the
// page, so filter URLs stay short and shareable (/c/makina?vehicle_fuel=DIESEL rather than a
// dozen empty parameters); the address bar shows the URL that was fetched. A currency means
// nothing without a price, so it goes too. The server ignores empty values anyway, which keeps
// the pages working without JavaScript.
document.addEventListener("turbo:before-fetch-request", (event) => {
  const { url, fetchOptions } = event.detail;
  if (!(event.target instanceof HTMLFormElement) || fetchOptions.method?.toUpperCase() !== "GET") return;
  const params = [...url.searchParams].filter(([, value]) => value.trim() !== "");
  const hasPrice = params.some(([name]) => name === "price_min" || name === "price_max");
  url.search = new URLSearchParams(params.filter(([name]) => hasPrice || name !== "currency")).toString();
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
document.addEventListener("touchstart", () => {}, { passive: true });
