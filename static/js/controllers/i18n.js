// Not a controller: the scripts' texts in the page's language (src/i18n.rs). Each is keyed by its
// Albanian, as the server's are; an English page carries their English (the JSON in #i18n-en,
// from i18n/en/js*.txt), and anything it lacks stays as it is.

let english;

export function t(text) {
  if (document.documentElement.lang !== "en") return text;
  english ??= (() => {
    try {
      return JSON.parse(document.getElementById("i18n-en")?.textContent ?? "{}");
    } catch {
      return {};
    }
  })();
  return english[text] ?? text;
}

// `t`, then its "{}" given the values in turn: tf(text, 3) for "… edhe {} foto".
export function tf(text, ...values) {
  let i = 0;
  return t(text).replace(/\{\}/g, () => String(values[i++] ?? ""));
}

// A page Turbo renders anew may be in the other language: read its texts afresh.
document.addEventListener("turbo:load", () => {
  english = undefined;
});
