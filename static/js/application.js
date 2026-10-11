// Turbo turns links and forms into fetch requests; Stimulus attaches small controllers to HTML.
import { FetchMethod, FetchRequest, session, visit } from "@hotwired/turbo";
import { Application } from "@hotwired/stimulus";
// What every page needs whatever controllers it names: the hearts' module asks with each page which
// listings are kept, for the header's heart too; the town's, to forget it (below); refresh.js, the
// filled-in fields of a GET form (below).
import HeartController from "controllers/heart_controller.js";
import FoldController from "controllers/fold_controller.js";
import { rememberSearch } from "controllers/recent_searches.js";
import PlaceController, { forgetTown } from "controllers/place_controller.js";
import { filledIn } from "controllers/refresh.js";

// The other controllers load as the page names them: the first element with data-controller="map"
// brings controllers/map_controller.js, and Stimulus then connects it to every such element, now and
// later. A page fetches only the controllers it uses; those its HTML names come preloaded with it
// (web::render), and a part of the page that comes later (a frame, a stream, the next page Turbo
// shows) brings its own. A module's address is in the import map, with the fingerprint.
const application = Application.start();
const requested = new Set(["heart", "place", "fold"]);
application.register("heart", HeartController);
application.register("place", PlaceController);
// A description's fold, at hand on every page, so that a listing's stand-in, which folds its
// description as the page does (listingStandIn), is folded before it is first drawn.
application.register("fold", FoldController);

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
  request.perform().then(() => {
    if (touched?.request === request) touched.arrived = true;
  }).catch(() => {});
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
    done.then(() => {
      const page = kept.get(url.href);
      if (page?.request === request) page.arrived = true;
    }).catch(() => {}).finally(() => {
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

// On a touch screen a tapped link's page slides in at once (base.css), as the user asked on
// 2026-10-11: "instant as soon as I click, fill the info as it comes". Where the page hasn't come
// yet (fetched ahead as the finger touched it or while it was in view, above, and arrived; or kept
// by Turbo from an earlier visit, which Turbo shows at once itself), Turbo is handed a stand-in to
// show meanwhile, a visit's snapshotHTML: it shows that as it shows a page it kept, a preview,
// sliding in, and the page itself takes its place as it comes, once the slide is over. The stand-in
// is this page's bar, its controllers left out, over grey shapes where the page's parts will be
// (base.css, .stand-in), and for a listing the listing page's own frame filled with what its card
// knows (listingStandIn, below). Turbo keeps no copy of it (turbo-cache-control). A link that names its own kind of visit (data-turbo-action)
// goes as Turbo takes it. As the listing comes, its first photo shows the card's copy, here
// already, softly blurred as in the stand-in, until its own has loaded over it (interim, the
// user's ask: "a lower resolution photo which you can use as blur on the next page").
const TOUCH = matchMedia("(pointer: coarse)");
let interim = null;
const arrived = (href) => (fresh() && touched.url === href && touched.arrived) || (keptAhead(href) && kept.get(href).arrived);
document.addEventListener("turbo:click", (event) => {
  const link = event.target;
  if (!TOUCH.matches || !(link instanceof HTMLAnchorElement) || link.closest("[data-turbo-action]")) return;
  const url = new URL(event.detail.url);
  if (!standable(link) || arrived(url.href) || session.view.snapshotCache.has(url)) return;
  event.preventDefault();
  event.detail.originalEvent?.preventDefault();
  visit(url.href, { snapshotHTML: standIn(link) });
});
document.addEventListener("turbo:before-render", ({ detail: { newBody } }) => {
  if (!interim || newBody.querySelector("[data-stand-in]")) return;
  const photo = newBody.querySelector(".gallery-photo img:not(.gallery-backdrop)");
  photo?.closest("picture, .gallery-photo > img")?.before(Object.assign(document.createElement("img"), { className: "gallery-interim", src: interim, alt: "" }));
  interim = null;
});
// A listing's photos that haven't come by the time its page shows come in whole, fading in over
// what stands in for them (the card's copy, blurred, or the photo's own tiny one behind it),
// rather than drawing themselves in from the top down as their bytes come, a sharp edge sweeping
// down the blurred copy (listing.css, .is-coming; the user's report of 2026-10-11, "a glitch when
// photo loads"). Marked as the page shows, before it is drawn, so that one the browser has at hand
// shows at once, untouched. A page that only changes in place keeps its photos as they are.
document.addEventListener("turbo:render", ({ detail: { renderMethod } }) => {
  if (renderMethod === "morph") return;
  for (const photo of document.querySelectorAll(".gallery-photo img:not(.gallery-backdrop, .gallery-interim)")) {
    if (photo.complete) continue;
    photo.classList.add("is-coming");
    const come = () => photo.classList.remove("is-coming");
    photo.addEventListener("load", come, { once: true });
    photo.addEventListener("error", come, { once: true });
  }
});

// A link a stand-in is shown for: one that leads the whole page to another of the site's pages
// by a plain GET (Turbo's own click, turbo:click, has left out the links it doesn't follow), not
// one that only changes this page's address (a filter's chip, a next page). Those not fetched
// ahead (data-turbo-prefetch="false", the report's link) have one too: they slid only once
// their page had come.
function standable(link) {
  if (link.pathname === location.pathname) return false;
  const method = link.getAttribute("data-turbo-method");
  if ((method && method.toLowerCase() !== "get") || link.matches("[data-turbo-confirm], [data-turbo-stream]")) return false;
  const frame = link.getAttribute("data-turbo-frame") ?? link.closest("turbo-frame")?.getAttribute("target");
  return !((link.hasAttribute("data-turbo-frame") || link.closest("turbo-frame")) && frame !== "_top");
}

function standIn(link) {
  const page = document.implementation.createHTMLDocument("");
  page.documentElement.lang = document.documentElement.lang;
  page.head.replaceWith(page.importNode(document.head, true));
  page.head.append(Object.assign(page.createElement("meta"), { name: "turbo-cache-control", content: "no-cache" }));
  for (const part of document.querySelectorAll("body > .language-offer, body > .site-header, body > .rail")) {
    const copy = page.importNode(part, true);
    for (const element of [copy, ...copy.querySelectorAll("[data-controller]")]) element.removeAttribute("data-controller");
    page.body.append(copy);
  }
  const card = link.closest(".card");
  // The bar as the page coming has it: a listing's with its search folded away behind the
  // magnifier (base.css, .search-folded), any other's with its search there, and none with the
  // front page's "Posto", its bar's alone (home.html, header_post).
  const bar = page.body.querySelector(".site-header");
  bar?.classList.remove("search-open");
  bar?.classList.toggle("search-folded", Boolean(card));
  bar?.querySelector(".header-post")?.remove();
  const main = Object.assign(page.createElement("main"), { className: "container" });
  main.setAttribute("aria-busy", "true");
  main.setAttribute("data-stand-in", "");
  interim = null;
  if (card) {
    main.innerHTML = listingStandIn(card, link);
    interim = main.querySelector(".gallery-interim")?.getAttribute("src") ?? null;
    const named = card.querySelector(".card-link")?.textContent.trim();
    if (named) page.title = `${named} | Pluribus`;
  } else {
    main.innerHTML = '<div class="stand-in"><span class="stand-in-line is-heading"></span><span class="stand-in-block"></span><span class="stand-in-line"></span><span class="stand-in-line is-short"></span><span class="stand-in-block"></span></div>';
  }
  page.body.append(main);
  return `<!DOCTYPE html>${page.documentElement.outerHTML}`;
}

// A listing's stand-in: the listing page's own frame (listing.html), which listing.css lays out as
// it lays out the page, so that the page fills it where it stands: its path's row with the steps'
// circles (its words the card's, data-up and data-here, the category above the listing's and its
// own, as the page says them); the photo the card shows, in the photo's frame (4:3 for a car or a property, data-deal,
// 6:5 for the others on a phone); the title the page heads itself with (a car's as its seller wrote
// it, data-heading); the line under it; a car's or a property's key facts; the price, with the
// loan calculator's circle at its line's end where the page has it (data-loan), and the circle
// that weighs the price after it, grey until the page gives it its verdict's colour
// (data-compare); the details
// as grey pills where the page has them under it (not a car's or a property's, whose seller comes
// there); the description, its start as the card has it; the contact bar at a phone's foot. What
// the card doesn't know are grey bars where the words will be (.stand-in-text, words about as
// long, unseen), which wrap as the page's do. The photo is the card's, softly blurred until the
// page's own comes (.gallery-interim). What the card knows of its page's words shows as words, as
// the user asked on 2026-10-11 ("load përshkrimi and the generated subheader", "the chat and phone
// number button as we already know their value"): the line under the title (data-line), its
// number whole on one line as the page has it; the description (data-description, its lines as
// they are), under the page's heading, folded by the page's own controller at a dozen lines with
// "Shfaq të gjithë përshkrimin" under them (fold_controller.js); and the contact bar's pills, the
// phone's or the e-mail's (data-reveal) and the message's (none where data-closed), their icons and
// words the page's own (#stand-in-parts, base.html).
function listingStandIn(card, link) {
  const escape = (text) => text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
  const unseen = (text) => `<span class="stand-in-text">${escape(text)}</span>`;
  const head = card.querySelector(".card-head");
  // The page's own parts, as the server words them (base.html, #stand-in-parts).
  const parts = document.getElementById("stand-in-parts")?.content;
  const part = (name) => parts?.querySelector(`[data-part="${name}"]`)?.outerHTML ?? "";
  const deal = Boolean(head?.hasAttribute("data-deal"));
  const named = card.querySelector(".card-link")?.textContent.trim() ?? "";
  const shown = card.querySelector("img.card-photo")?.currentSrc;
  const amount = card.querySelector(".card-amount");
  const price = amount?.firstChild?.textContent.trim() ?? "";
  // Under the price what qualifies it, as the page says it: without its brackets, on a line of its
  // own (price_note_alone, listings.rs).
  const said = [...(amount?.querySelectorAll(".card-note") ?? [])].map((note) => note.textContent.trim()).join(" ");
  const notes = said.replace(/[()]/g, "").trim();
  const number = link.pathname.match(/^\/listing\/(\d+)/)?.[1] ?? "";
  const place = card.querySelector(".card-place")?.textContent.trim() || "Tiranë";
  // The line under the title, the page's own words as the card has them (data-line), or about as
  // long as the page's: the made title before it where the seller's heads the page, what kind and
  // in what state, and on a car's or a property's page where and when, before the number.
  const kind = "Në shitje · E përdorur";
  const line = head?.dataset.line ?? (deal ? `${head?.dataset.heading ? `${named} · ` : ""}${kind} · ${place} · 00.00.2026 · Nr. ${number}` : `${kind} · Numri i njoftimit: ${number}`);
  // The page's own line in words, its last part, the number, whole on one line (.listing-number).
  const end = line.lastIndexOf(" · ");
  const spoken = head?.dataset.line && end > 0 ? `${escape(line.slice(0, end))} · <span class="listing-number">${escape(line.slice(end + 3))}</span>` : unseen(line);
  // As many key facts as the page has (data-facts), two to a line on a phone; none but on a car's
  // or a property's page.
  const facts = deal ? Number(head.dataset.facts ?? 4) : 0;
  const fact = `<div class="key-fact"><span class="key-fact-icon"></span><div class="key-fact-text"><dt>${unseen("Kilometrazhi")}</dt><dd>${unseen("000 000 km")}</dd></div></div>`;
  const photo = shown ? `<img class="gallery-backdrop" src="${escape(shown)}" alt=""><img class="gallery-interim" src="${escape(shown)}" alt="">` : "";
  // The path as the page has it: the way back (its chevron this page's own, where it has one), the
  // category above, the sign between, the listing's own.
  const chevron = document.querySelector(".breadcrumb-back svg")?.outerHTML ?? "";
  const path = head?.dataset.here
    ? `<a class="breadcrumb-back">${chevron}</a><a class="breadcrumb-up">${escape(head.dataset.up ?? "")}</a><span class="breadcrumb-sep" aria-hidden="true">›</span><a class="breadcrumb-here">${escape(head.dataset.here)}</a>`
    : unseen("Automjete › Makina");
  const note = notes ? `<span class="price-note">${escape(notes)}</span>` : "";
  const pills = ["Samsung", "Galaxy A15", "128 GB", "E re", "Garanci"].map((words) => `<dd class="attribute">${escape(words)}</dd>`).join("");
  const details = deal ? "" : `<section class="listing-details"><h2>${unseen("Detajet")}</h2><dl class="attributes"><div class="attribute-group">${pills}</div></dl></section>`;
  const words = head?.dataset.description ?? card.querySelector(".card-description-text")?.textContent.trim();
  const description = words
    ? `<section class="listing-description">${part("description") || `<h2>${unseen("Përshkrimi")}</h2>`}`
      + `<div class="description-fold" data-controller="fold"><p class="description" data-fold-target="text">${escape(words)}</p>${part("more")}</div></section>`
    : "";
  // The contact bar's pills as the page has them; none where the page has none.
  const reveal = head?.dataset.reveal ? part(head.dataset.reveal) : "";
  const message = head?.hasAttribute("data-closed") ? "" : part("message");
  const contact = !parts
    ? '<aside class="contact-bar"><span class="button secondary reveal-button"></span><span class="button message-button"></span></aside>'
    : reveal || message ? `<aside class="contact-bar">${reveal}${message}</aside>` : "";
  return `<div class="listing-page"><article class="listing ${deal ? "listing-car" : "listing-split"} stand-in-listing">`
    + `<div class="listing-tags"><nav class="breadcrumb listing-path">${path}</nav>`
    + '<nav class="listing-steps"><span class="back-button list-button"></span><span class="back-button step-button"></span><span class="back-button step-button"></span></nav></div>'
    + `<div class="listing-media"><div class="gallery"><div class="listing-photos"><div class="gallery-photo">${photo}</div></div></div></div>`
    + `<div class="listing-side"><header class="listing-header"><h1>${escape(head?.dataset.heading || named)}</h1><p class="card-meta listing-kind">${spoken}</p></header>`
    + (facts > 0 ? `<dl class="key-facts">${fact.repeat(facts)}</dl>` : "")
    + `<div class="listing-deal"><div class="price-box"><div class="price-row"><p class="listing-price"><span class="price-amount">${escape(price)}</span>${note}</p>`
    + `${head?.hasAttribute("data-loan") ? '<details class="bubble loan" inert><summary></summary></details>' : ""}`
    + `${head?.hasAttribute("data-compare") ? '<details class="bubble car-price" inert><summary></summary></details>' : ""}</div></div></div>`
    + `${details}${description}</div>`
    + contact
    + "</article></div>";
}

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
// Where the browser has animated the change itself, as Android's Chrome and iOS do under a swipe
// from the screen's edge, that would be a second change, from the page left to the page come back
// to, and the page seems to jump: such a visit goes without it, as do those in browsers that can't
// tell (Safari's engine, which every iPhone browser runs on, Firefox). But a press of the page's
// own way back (back_controller.js, which says so first: back:button), which no browser animates:
// that one slides back everywhere, where it didn't on an iPhone. The visit's view transition is
// skipped as it starts (unanimated), the setting itself left in the page. Until 2026-10-11 the
// setting was taken out until the page come back to brought it back with its head, which left it
// out of the copy Turbo keeps of the page left, and that page, opened again, came at once, without
// its slide (on Android, where the user found it: "sliding still doesn't work").
let backByButton = -Infinity;
let unanimated = false;
document.addEventListener("back:button", () => { backByButton = performance.now(); });
addEventListener("popstate", (event) => {
  const byButton = performance.now() - backByButton < 1000;
  backByButton = -Infinity;
  unanimated = Boolean(event.state?.turbo) && !byButton && (event.hasUAVisualTransition ?? true);
});
// Turbo's visit back has begun as the popstate comes here; any other visit slides as ever.
document.addEventListener("turbo:visit", ({ detail: { action } }) => {
  if (action !== "restore") unanimated = false;
});
const startViewTransition = document.startViewTransition?.bind(document);
if (startViewTransition) {
  document.startViewTransition = (update) => {
    const transition = startViewTransition(update);
    if (unanimated) {
      unanimated = false;
      transition.ready.catch(() => {});
      transition.skipTransition();
    }
    return transition;
  };
}
