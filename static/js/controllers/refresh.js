import { visit } from "@hotwired/turbo";

// Not a controller: what the filters' and the map's controllers share with application.js.

// The address a refresh is fetching, until the page it brings has loaded: a field left and then
// its card closed ask for it twice.
let refreshing = null;
document.addEventListener("turbo:load", () => { refreshing = null; });

// What a GET form's address carries: its fields filled in only, so filter addresses stay short and
// shareable (/c/makina?vehicle_fuel=DIESEL rather than a dozen empty parameters). A currency means
// nothing without a price, so it goes too. The server ignores empty values anyway, which keeps the
// pages working without JavaScript.
export function filledIn(params) {
  const filled = [...params].filter(([, value]) => typeof value === "string" && value.trim() !== "");
  const hasPrice = filled.some(([name]) => name === "price_min" || name === "price_max");
  return new URLSearchParams(filled.filter(([name]) => hasPrice || name !== "currency"));
}

// The page again for a GET form as it is now (the filters, the map's point), without leaving it:
// Turbo fetches the form's address and morphs the page into the new one, the scroll kept and the
// address replaced rather than added to the history. It caches nothing, so nothing folds for a
// cache (turbo:before-cache), and the controllers keep what is open as it is while the page
// morphs (turbo:before-morph-element and turbo:before-morph-attribute). It needs no cross-fade
// (the view transitions base.html asks for), which would also hold the page still for a moment,
// taps and all, so the setting goes until the new page's head brings it back. An address the page
// already has, or is already fetching (refreshing), needs nothing.
export function refreshFor(form) {
  const url = new URL(form.action);
  url.search = filledIn(new FormData(form)).toString();
  if (sameAddress(url, new URL(refreshing ?? location.href))) return;
  refreshing = url.href;
  document.head.querySelector('meta[name="view-transition"]')?.remove();
  visit(url.href, { action: "replace", shouldCacheSnapshot: false, refresh: { method: "morph", scroll: "preserve" } });
}

// The same page with the same fields, in whatever order.
function sameAddress(a, b) {
  const key = (url) => {
    const params = new URLSearchParams(url.search);
    params.sort();
    return `${url.pathname}?${params}`;
  };
  return key(a) === key(b);
}
