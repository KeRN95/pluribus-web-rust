// Turbo turns links and forms into fetch requests; Stimulus attaches small controllers to HTML.
import { FetchMethod, FetchRequest } from "@hotwired/turbo";
import { Application } from "@hotwired/stimulus";
// What every page needs whatever controllers it names: the hearts' module asks with each page which
// listings are kept, for the header's heart too; the town's, to forget it (below); refresh.js, the
// filled-in fields of a GET form (below).
import HeartController from "controllers/heart_controller.js";
import { rememberSearch } from "controllers/recent_searches.js";
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

// A tapped card keeps its title underlined in the accent (page.css, .card.pressed) while its
// listing loads, and a tapped category's pill its icon filled with it and its name underlined
// (welcome.css, .shortcut.pressed); the copy of the page Turbo keeps for going back loses them.
// Only once the page has taken PRESS_AFTER to come, as the user asked on 2026-10-11: one fetched
// ahead (below) shows at once, sliding in on a touch screen (base.css), with no mark and no wait
// for one (the next page was held 200 ms after a tap so that the mark was seen). Only a card's
// links open it: a big card's arrows step through its photos. iOS only shows :active where a
// touch listener is, as the one here is.
const PRESS_AFTER = 100;
let pressing = null;
document.addEventListener("touchstart", () => {}, { passive: true });
document.addEventListener("turbo:click", (event) => {
  const pressed = event.target.closest(".card a")?.closest(".card") ?? event.target.closest("a.shortcut");
  clearTimeout(pressing);
  if (pressed) pressing = setTimeout(() => pressed.classList.add("pressed"), PRESS_AFTER);
});
document.addEventListener("turbo:before-render", () => clearTimeout(pressing));
document.addEventListener("turbo:before-cache", () => {
  for (const pressed of document.querySelectorAll(".card.pressed, .shortcut.pressed")) pressed.classList.remove("pressed");
});

// iPhones and iPads zoom into a field whose words are under 16px as it takes the focus, which the
// page made smaller (data-page-size) makes of every field. There, and only there, the viewport's
// maximum-scale=1 stops it, while Safari still lets a person pinch to zoom (it has ignored the
// limit for a finger since iOS 10; Android's Chrome would forbid pinching, so it isn't set there).
// Turbo puts each page's own viewport tag back as it shows it, so it is set again after each.
const IOS = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.userAgent.includes("Macintosh") && navigator.maxTouchPoints > 1);
function keepFieldsUnzoomed() {
  const viewport = document.querySelector('meta[name="viewport"]');
  if (IOS && viewport && !viewport.content.includes("maximum-scale")) viewport.content += ", maximum-scale=1";
}
keepFieldsUnzoomed();
document.addEventListener("turbo:load", keepFieldsUnzoomed);

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
  if ((fresh() && touched.url === url.href) || keptAhead(url.href)) return;
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
  if (touching) return;
  const { url, fetchOptions } = event.detail;
  if (fetchOptions.method !== "GET") return;
  if (touched && fresh() && url.href === touched.url) event.detail.fetchRequest = touched.request;
  else if (keptAhead(url.href)) event.detail.fetchRequest = kept.get(url.href).request;
  touched = null;
  kept.delete(url.href);
}, true);

// A finger gives a tap's page only the tap's own length of a head start, about a tenth of a
// second, where a mouse resting on a link gives a click's most of a second; and a page's round
// trip is longer than a tap (some 150 ms through the site's tunnel). So on a touch screen the
// page behind a link is fetched ahead once the link has been on screen for 75 ms: one the
// visitor stops on, not every one a swipe flicks past (a link that leaves sooner starts its wait
// again when it comes back). The cards' listings, the categories' pills (the welcome page's, a
// category page's path and subcategories), any link Turbo would fetch ahead under a mouse
// (aheadable), two at a time, the next as one finishes, at most twelve a page, quietly (priority
// low), each kept half a minute for the visit a tap makes (above, as the touch's own). Not where
// the visitor asks to save data, nor on a connection the browser calls slow
// (navigator.connection), nor a page kept already, nor a waiting link that has left the screen.
const AHEAD_AFTER = 75;
const AHEAD_IN_VIEW = ".card-link, .shortcut";
const kept = new Map();
const keptAhead = (href) => {
  const page = kept.get(href);
  if (page && Date.now() - page.at < 30_000) return true;
  kept.delete(href);
  return false;
};
const connection = navigator.connection;
const dataIsCheap = () => !connection?.saveData && (!connection?.effectiveType || connection.effectiveType === "4g");
let inView = null;
let waiting = [];
let fetching = 0;
let left = 0;
// The links on screen, each with the timer that queues it once it has stayed AHEAD_AFTER.
const onScreen = new Map();
function fetchAhead() {
  while (fetching < 2 && left > 0 && waiting.length) {
    const link = waiting.shift();
    if (!onScreen.has(link)) continue;
    inView?.unobserve(link);
    onScreen.delete(link);
    const url = new URL(link.href);
    if (!aheadable(link) || keptAhead(url.href) || (touched && fresh() && touched.url === url.href)) continue;
    const request = new FetchRequest(ahead, FetchMethod.get, url, new URLSearchParams(), link);
    request.fetchOptions.priority = "low";
    kept.set(url.href, { request, at: Date.now() });
    left--;
    fetching++;
    touching = true;
    const done = request.perform();
    touching = false;
    done.catch(() => {}).finally(() => {
      fetching--;
      fetchAhead();
    });
  }
}
function stopWatching() {
  inView?.disconnect();
  inView = null;
  waiting = [];
  for (const timer of onScreen.values()) clearTimeout(timer);
  onScreen.clear();
}
function watchLinksInView() {
  stopWatching();
  if (!matchMedia("(pointer: coarse)").matches || !dataIsCheap()) return;
  left = 12;
  inView = new IntersectionObserver((entries) => {
    for (const { target: link, isIntersecting } of entries) {
      if (isIntersecting && !onScreen.has(link)) {
        onScreen.set(link, setTimeout(() => {
          waiting.push(link);
          fetchAhead();
        }, AHEAD_AFTER));
      } else if (!isIntersecting && onScreen.has(link)) {
        clearTimeout(onScreen.get(link));
        onScreen.delete(link);
        waiting = waiting.filter((other) => other !== link);
      }
    }
  });
  for (const link of document.querySelectorAll(AHEAD_IN_VIEW)) {
    if (link.matches("a[href]") && aheadable(link)) inView.observe(link);
  }
}
document.addEventListener("turbo:load", watchLinksInView);
document.addEventListener("turbo:before-visit", stopWatching);

// A listing's page, once Turbo shows it, says it was seen (listings.rs, seen), for the welcome
// page's "Më të klikuarat"; the kept listings' page, that the price drops it shows were seen
// (saved.rs, seen): not when Turbo only fetched them ahead, under the pointer.
document.addEventListener("turbo:load", () => {
  const url = document.querySelector("[data-seen-url]")?.dataset.seenUrl;
  if (url) navigator.sendBeacon(url);
});

// A listing's page shown goes first among those the visitor saw lately, the cookie whose
// numbers the welcome page shows in a row (browse.rs, SEEN_COOKIE, SEEN_MOST at most, kept 90
// days); not the seller's own, which has no number to give. A search with words shown goes
// first among the last searches, which a search box offers while empty (recent_searches.js).
const SEEN_COOKIE = "shikuar";
const SEEN_MOST = 12;
document.addEventListener("turbo:load", () => {
  const number = document.querySelector("[data-listing-number]")?.dataset.listingNumber;
  if (number) {
    const pair = document.cookie.split("; ").find((each) => each.startsWith(`${SEEN_COOKIE}=`));
    const before = (pair ?? "").slice(SEEN_COOKIE.length + 1).split(".").filter((each) => /^\d+$/.test(each) && each !== number);
    const numbers = [number, ...before].slice(0, SEEN_MOST).join(".");
    document.cookie = `${SEEN_COOKIE}=${numbers}; path=/; max-age=${90 * 24 * 60 * 60}; samesite=lax`;
  }
  const searched = document.querySelector("[data-search-words]");
  if (searched) rememberSearch(searched.dataset.searchWords, searched.dataset.searchAddress);
});

// Going back or forward, Turbo shows its copy of the page with the change base.html asks for
// (view transitions: a cross-fade, on a touch screen the page sliding away as a drawer, base.css).
// Where the browser has animated the change itself, as iOS does under a swipe
// from the screen's edge, that would be a second change, from the page left to the page come back
// to, and the page seems to jump: such a visit goes without it, as do those in browsers that can't
// tell. The page shown brings the setting back with its head, as after a refresh (refresh.js).
addEventListener("popstate", (event) => {
  if (event.state?.turbo && (event.hasUAVisualTransition ?? true)) {
    document.head.querySelector('meta[name="view-transition"]')?.remove();
  }
});
