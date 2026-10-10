// The visitor's last searches with words, the latest first: what each searched and the page it
// searched on (the address "Ruaj kërkimin" keeps, its order left out), each page once, RECENT_MOST
// at most, kept in the browser alone (localStorage, STORE). A search shown goes first
// (application.js, from its results' data-search-words); an empty search box offers them, and
// forgets them when asked (suggest_controller.js). A browser that won't keep them has none.
const STORE = "recent-searches";
const RECENT_MOST = 6;

// Only an address of the site's own: what the browser kept could have been changed by hand.
function local(address) {
  return typeof address === "string" && address.startsWith("/") && !address.startsWith("//");
}

export function recentSearches() {
  try {
    const kept = JSON.parse(localStorage.getItem(STORE));
    return Array.isArray(kept) ? kept.filter((search) => typeof search?.words === "string" && local(search.address)) : [];
  } catch {
    return [];
  }
}

export function rememberSearch(words, address) {
  if (!words?.trim() || !local(address)) return;
  const searches = [{ words: words.trim(), address }, ...recentSearches().filter((search) => search.address !== address)];
  try {
    localStorage.setItem(STORE, JSON.stringify(searches.slice(0, RECENT_MOST)));
  } catch {}
}

export function forgetSearches() {
  try {
    localStorage.removeItem(STORE);
  } catch {}
}
