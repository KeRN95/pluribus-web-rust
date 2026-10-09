// Turbo turns links and forms into fetch requests; Stimulus attaches small controllers to HTML.
import { FetchMethod, FetchRequest } from "@hotwired/turbo";
import { Application } from "@hotwired/stimulus";
// What every page needs whatever controllers it names: the hearts' module asks with each page which
// listings are kept, for the header's heart too; the town's, to forget it (below); refresh.js, the
// filled-in fields of a GET form (below).
import HeartController from "controllers/heart_controller.js";
import PlaceController, { forgetTown } from "controllers/place_controller.js";
import { filledIn } from "controllers/refresh.js";

// The other controllers load as the page names them: the first element with data-controller="map"
// brings controllers/map_controller.js, and Stimulus then connects it to every such element, now and
// later. A page fetches only the controllers it uses; those its HTML names come preloaded with it
// (web::render), and a part of the page that comes later (a frame, a stream, the next page Turbo
// shows) brings its own. A module's address is in the import map, with the fingerprint.
const application = Application.start();
const requested = new Set(["heart", "place"]);
application.register("heart", HeartController);
application.register("place", PlaceController);

function load(identifier) {
  if (requested.has(identifier)) return;
  requested.add(identifier);
  import(`controllers/${identifier.replaceAll("-", "_")}_controller.js`)
    .then((module) => application.register(identifier, module.default))
    .catch((error) => {
      requested.delete(identifier);
      console.error(`The "${identifier}" controller did not load`, error);
    });
}

function loadWithin(element) {
  const named = element.querySelectorAll("[data-controller]");
  for (const each of element.matches("[data-controller]") ? [element, ...named] : named) {
    for (const identifier of each.dataset.controller.split(/\s+/)) if (identifier) load(identifier);
  }
}

loadWithin(document.documentElement);
new MutationObserver((mutations) => {
  for (const mutation of mutations) {
    if (mutation.type === "attributes") loadWithin(mutation.target);
    for (const node of mutation.addedNodes) if (node.nodeType === Node.ELEMENT_NODE) loadWithin(node);
  }
}).observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-controller"] });

// A link that drops the town (the town's chip, "Kërko në gjithë Shqipërinë") forgets it too, before
// Turbo follows it, so the page it opens doesn't open in it again (place_controller.js). Such links are
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
// the finger lifts. So a link starts fetching the moment a finger touches it, any link Turbo would
// fetch ahead under a mouse (aheadable), and the visit the tap then makes takes that response, as
// it takes Turbo's own (turbo:before-fetch-request, fetchRequest); a finger that moves, scrolling,
// drops it. A page fetched ahead changes nothing: a listing's view, price drops seen and messages
// read count once a page shows (below).
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
// What Turbo fetches ahead under a mouse (LinkPrefetchObserver in turbo-8.0.23.js): a link to
// another page of this site, not opened elsewhere nor downloaded, nobody opted out of
// (data-turbo-prefetch="false", data-turbo="false"), that only reads (no method other than GET, no
// confirm, no stream); and, here, one that leads the whole page, not a frame inside it, whose
// requests ask for the frame alone.
function aheadable(link) {
  if (!link.getAttribute("href") || link.origin !== location.origin || !["http:", "https:"].includes(link.protocol)) return false;
  if (link.hasAttribute("target") || link.hasAttribute("download")) return false;
  if (link.pathname + link.search === location.pathname + location.search) return false;
  if (link.closest('[data-turbo="false"]') || link.closest("[data-turbo-prefetch]")?.dataset.turboPrefetch === "false") return false;
  const method = link.getAttribute("data-turbo-method");
  if (method && method.toLowerCase() !== "get") return false;
  if (link.matches("[data-turbo-confirm], [data-turbo-stream], [data-remote], [data-behavior], [data-confirm], [data-method]")) return false;
  const frame = link.getAttribute("data-turbo-frame") ?? link.closest("turbo-frame")?.getAttribute("target");
  if ((link.hasAttribute("data-turbo-frame") || link.closest("turbo-frame")) && frame !== "_top") return false;
  return link.dispatchEvent(new CustomEvent("turbo:before-prefetch", { bubbles: true, cancelable: true }));
}
document.addEventListener("touchstart", (event) => {
  const link = event.touches.length === 1 && event.target.closest?.("a[href]");
  if (!link || !aheadable(link)) return;
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
