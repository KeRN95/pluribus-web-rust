import { Controller } from "@hotwired/stimulus";

// Trying typefaces and icons (TYPE_TRIAL=true in .env; base.html, .type-menu): the top bar's "Aa"
// panel picks the headings' typeface and the text's from the list below, the site's own three and
// Google Fonts', and sets their weights, the headings' (h1) and the subheadings' (h2), the text's
// and its strong's (labels, buttons, card titles); and an icon library in place of the site's own
// drawings, for the icons that have a name in it (ICONS, LIBRARIES below), fetched from unpkg as
// picked. What is picked goes on the page at once, as the root's custom properties (base.css reads
// --font-heading, --font, --heading-weight, --subheading-weight, --regular and --semibold) and as
// the icons' <svg> insides, and stays, kept in this browser (localStorage) and put back on every
// page; a Google font's stylesheet is fetched once, as it is picked. "Rikthe" puts the site's own
// back. For the visitor's own browser alone.
const KEY = "type-trial";
// Name, Google Fonts' weight range (null for a font the site serves itself), the fallbacks.
const FONTS = [
  ["Geist", null, 'system-ui, -apple-system, "Segoe UI", Roboto, Arial, sans-serif'],
  ["Inter", null, '"Geist", system-ui, sans-serif'],
  ["Playfair Display", null, "Georgia, serif"],
  ["Instrument Sans", "400..700", "sans-serif"],
  ["Hanken Grotesk", "100..900", "sans-serif"],
  ["Familjen Grotesk", "400..700", "sans-serif"],
  ["Schibsted Grotesk", "400..900", "sans-serif"],
  ["DM Sans", "100..1000", "sans-serif"],
  ["Manrope", "200..800", "sans-serif"],
  ["Onest", "100..900", "sans-serif"],
  ["Figtree", "300..900", "sans-serif"],
  ["Plus Jakarta Sans", "200..800", "sans-serif"],
  ["Archivo", "100..900", "sans-serif"],
  ["Source Serif 4", "200..900", "Georgia, serif"],
  ["Literata", "200..900", "Georgia, serif"],
  ["Fraunces", "100..900", "Georgia, serif"],
  ["Newsreader", "200..800", "Georgia, serif"],
  ["Lora", "400..700", "Georgia, serif"],
];
const DEFAULTS = {
  headingFont: "Inter",
  headingWeight: 700,
  subheadingWeight: 700,
  textFont: "Geist",
  textWeight: 400,
  strongWeight: 600,
  iconLibrary: "pluribus",
};

// The icon libraries, each by its files on unpkg: Lucide and Tabler in strokes in a 24-unit box as
// the site's own are, Heroicons the same but drawn at 1.5, Phosphor filled in a 256-unit box. An
// icon's inside goes into the site's <svg> in place of its own, with the box and the attributes
// the library draws with.
const LIBRARIES = {
  lucide: { url: (name) => `https://unpkg.com/lucide-static@0.460.0/icons/${name}.svg`, column: 0 },
  tabler: { url: (name) => `https://unpkg.com/@tabler/icons@3.31.0/icons/outline/${name}.svg`, column: 1 },
  phosphor: {
    url: (name) => `https://unpkg.com/@phosphor-icons/core@2.1.1/assets/regular/${name}.svg`,
    column: 2,
    attributes: { viewBox: "0 0 256 256", fill: "currentColor", stroke: "none" },
    // Filled: said in the element's own style too, which the stylesheet's rules for a stroked
    // icon (a card's heart: fill transparent, a white stroke) would otherwise undo, leaving a
    // hairline of the 256-unit box.
    style: { fill: "currentColor", stroke: "none" },
  },
  heroicons: { url: (name) => `https://unpkg.com/heroicons@2.2.0/24/outline/${name}.svg`, column: 3, attributes: { "stroke-width": "1.5" } },
};
// Each icon the trial swaps, by its key (the site's data-icon, or a category's slug below, as
// Lucide names it): its name in Lucide, Tabler, Phosphor and Heroicons; null where a library has
// none, and the site's own stays.
const ICONS = {
  "user": ["user", "user", "user", "user"],
  "chevron-left": ["chevron-left", "chevron-left", "caret-left", "chevron-left"],
  "chevron-right": ["chevron-right", "chevron-right", "caret-right", "chevron-right"],
  "chevron-down": ["chevron-down", "chevron-down", "caret-down", "chevron-down"],
  "search": ["search", "search", "magnifying-glass", "magnifying-glass"],
  "sliders-horizontal": ["sliders-horizontal", "adjustments-horizontal", "sliders-horizontal", "adjustments-horizontal"],
  "map-pin": ["map-pin", "map-pin", "map-pin", "map-pin"],
  "trending-up": ["trending-up", "trending-up", "trend-up", "arrow-trending-up"],
  "arrow-up-down": ["arrow-up-down", "arrows-sort", "arrows-down-up", "arrows-up-down"],
  "layout-grid": ["layout-grid", "layout-grid", "squares-four", "squares-2x2"],
  "square": ["square", "square", "square", null],
  "list": ["list", "list", "list", "list-bullet"],
  "plus": ["plus", "plus", "plus", "plus"],
  "heart": ["heart", "heart", "heart", "heart"],
  "message-circle": ["message-circle", "message-circle", "chat-circle", "chat-bubble-oval-left"],
  "moon": ["moon", "moon", "moon", "moon"],
  "sun": ["sun", "sun", "sun", "sun"],
  "eye": ["eye", "eye", "eye", "eye"],
  "phone": ["phone", "phone", "phone", "phone"],
  "link": ["link", "link", "link", "link"],
  "check": ["check", "check", "check", "check"],
  "flag": ["flag", "flag", "flag", "flag"],
  "x": ["x", "x", "x", "x-mark"],
  "send": ["send", "send", "paper-plane-right", "paper-airplane"],
  "car": ["car", "car", "car", null],
  "car-front": ["car-front", "car", "car-profile", null],
  "motorbike": [null, "motorbike", "motorcycle", null],
  "truck": ["truck", "truck", "truck", "truck"],
  "wrench": ["wrench", "tool", "wrench", "wrench"],
  "house": ["house", "home", "house", "home"],
  "building": ["building", "building", "buildings", "building-office"],
  "bed": ["bed", "bed", "bed", null],
  "store": ["store", "building-store", "storefront", "building-storefront"],
  "building-2": ["building-2", "building-skyscraper", "buildings", "building-office-2"],
  "warehouse": ["warehouse", "building-warehouse", "warehouse", null],
  "smartphone": ["smartphone", "device-mobile", "device-mobile", "device-phone-mobile"],
  "laptop": ["laptop", "device-laptop", "laptop", null],
  "monitor": ["monitor", "device-desktop", "monitor", "computer-desktop"],
  "tablet": ["tablet", "device-tablet", "device-tablet", "device-tablet"],
  "tv": ["tv", "device-tv", "television", "tv"],
  "gamepad-2": ["gamepad-2", "device-gamepad-2", "game-controller", null],
  "camera": ["camera", "camera", "camera", "camera"],
  "headphones": ["headphones", "headphones", "headphones", null],
  "sofa": ["sofa", "sofa", "couch", null],
  "armchair": ["armchair", "armchair", "armchair", null],
  "washing-machine": ["washing-machine", "wash-machine", "washing-machine", null],
  "sprout": ["sprout", "plant", "plant", null],
  "lamp": ["lamp", "lamp", "lamp", null],
  "briefcase": ["briefcase", "briefcase", "briefcase", "briefcase"],
  "utensils": ["utensils", "tools-kitchen-2", "fork-knife", null],
  "hard-hat": ["hard-hat", "helmet", "hard-hat", null],
  "stethoscope": ["stethoscope", "stethoscope", "stethoscope", null],
  "code": ["code", "code", "code", "code-bracket"],
  "tag": ["tag", "tag", "tag", "tag"],
  "headset": ["headset", "headset", "headset", null],
  "hammer": ["hammer", "hammer", "hammer", null],
  "shirt": ["shirt", "shirt", "t-shirt", null],
  "dumbbell": ["dumbbell", "barbell", "barbell", null],
  "paw-print": ["paw-print", "paw", "paw-print", null],
  "factory": ["factory", "building-factory-2", "factory", "building-office-2"],
  "package": ["package", "package", "package", "cube"],
  "navigation": ["navigation", "navigation", "navigation-arrow", null],
  "zap": ["zap", "bolt", "lightning", "bolt"],
  "speaker": ["speaker", "speaker", "speaker-high", "speaker-wave"],
};
// The categories' icons by their slug (the pill's link, /c/{slug}; the page's own, its path).
const SLUGS = {
  "automjete": "car", "prona": "house", "elektronike": "smartphone", "shtepi-dhe-kopsht": "sofa", "pune": "briefcase",
  "sherbime": "hammer", "veshje-dhe-aksesore": "shirt", "sport-dhe-hobi": "dumbbell", "kafshe-shtepiake": "paw-print",
  "makineri-dhe-biznes": "factory", "te-tjera": "package",
  "makina": "car-front", "motocikleta": "motorbike", "automjete-tregtare": "truck", "pjese-dhe-aksesore": "wrench",
  "audio-dhe-video": "speaker", "gps-dhe-navigatore": "navigation", "mbushes-per-makina-elektrike": "zap", "pjese-te-tjera": "package",
  "apartamente": "building", "garsoniera": "bed", "dyqane": "store", "zyra": "building-2", "garazhe": "warehouse",
  "telefona": "smartphone", "laptope": "laptop", "kompjutere": "monitor", "tableta": "tablet", "televizore": "tv",
  "konsola-lojerash": "gamepad-2", "aparate-fotografike": "camera", "audio-dhe-kufje": "headphones",
  "mobilje": "armchair", "elektroshtepiake": "washing-machine", "kopsht": "sprout", "dekor": "lamp",
  "pune-turizem": "utensils", "pune-ndertim": "hard-hat", "pune-shendetesi": "stethoscope", "pune-it": "code",
  "pune-shitje": "tag", "pune-call-center": "headset", "riparime": "wrench",
};
const VIEWS = { "cards-icon-small": "layout-grid", "cards-icon-big": "square", "cards-icon-list": "list" };
// Each library's files, as fetched: null for one it hasn't.
const fetched = new Map();
// Each <svg> as the site drew it, to put back.
const own = new WeakMap();
const ATTRIBUTES = ["viewBox", "fill", "stroke", "stroke-width"];
const STYLED = ["fill", "stroke"];

// The key of an <svg> the trial swaps: its data-icon; a category pill's, by its link's slug; the
// page's own pill's, by the page's path; a view's, by its class. None for the rest.
function keyOf(svg) {
  if (svg.dataset.icon) return svg.dataset.icon;
  if (svg.classList.contains("shortcut-icon")) {
    const link = svg.closest("a[href]");
    const path = link ? new URL(link.href, location.href).pathname : location.pathname;
    const slug = path.match(/^\/c\/([^/?]+)/)?.[1];
    return slug && SLUGS[slug];
  }
  for (const [className, key] of Object.entries(VIEWS)) {
    if (svg.classList.contains(className)) return key;
  }
  return null;
}

function restore(svg) {
  const drawn = own.get(svg);
  if (!drawn) return;
  svg.innerHTML = drawn.inner;
  for (const [at, value] of Object.entries(drawn.attributes)) {
    if (value === null) svg.removeAttribute(at);
    else svg.setAttribute(at, value);
  }
  for (const property of STYLED) svg.style[property] = "";
  delete svg.dataset.drawnBy;
  own.delete(svg);
}

// A library's file for `name`, once: its inside and its box, or null where it has none.
function fetchIcon(library, name) {
  const url = library.url(name);
  if (!fetched.has(url)) {
    fetched.set(
      url,
      fetch(url)
        .then((response) => (response.ok ? response.text() : null))
        .then((text) => {
          if (!text) return null;
          const parsed = new DOMParser().parseFromString(text, "image/svg+xml").documentElement;
          if (parsed.tagName !== "svg") return null;
          return { inner: parsed.innerHTML, viewBox: parsed.getAttribute("viewBox") ?? "0 0 24 24" };
        })
        .catch(() => null),
    );
  }
  return fetched.get(url);
}

export default class extends Controller {
  static targets = ["field"];

  connect() {
    this.state = { ...DEFAULTS, ...this.saved() };
    for (const select of this.fieldTargets.filter((field) => field.name.endsWith("Font"))) {
      select.replaceChildren(...FONTS.map(([name]) => new Option(name, name)));
    }
    this.fill();
    this.apply();
    // Cards that come later (the next page, a row's) get the library's icons too.
    this.arrivals = new MutationObserver(() => this.applyIcons());
    this.arrivals.observe(document.body, { childList: true, subtree: true });
  }

  disconnect() {
    this.arrivals?.disconnect();
  }

  change(event) {
    const { name, value } = event.target;
    this.state[name] = event.target.type === "range" ? Number(value) : value;
    this.fill();
    this.apply();
    try {
      localStorage.setItem(KEY, JSON.stringify(this.state));
    } catch {
      // Nothing kept: the choice holds for this page.
    }
  }

  reset(event) {
    event.preventDefault();
    this.state = { ...DEFAULTS };
    try {
      localStorage.removeItem(KEY);
    } catch {
      // Nothing was kept.
    }
    this.fill();
    this.apply();
  }

  saved() {
    try {
      return JSON.parse(localStorage.getItem(KEY)) ?? {};
    } catch {
      return {};
    }
  }

  // The fields say what is set; a slider's number stands after it.
  fill() {
    for (const field of this.fieldTargets) {
      field.value = this.state[field.name];
      const shown = field.nextElementSibling;
      if (shown?.tagName === "OUTPUT") shown.value = field.value;
    }
  }

  apply() {
    this.applyIcons();
    const root = document.documentElement.style;
    const { headingFont, textFont } = this.state;
    const isDefault = Object.entries(DEFAULTS).every(([key, value]) => key === "iconLibrary" || this.state[key] === value);
    if (isDefault) {
      for (const property of ["--font-heading", "--font", "--heading-weight", "--subheading-weight", "--regular", "--semibold"]) {
        root.removeProperty(property);
      }
      return;
    }
    this.load(headingFont);
    this.load(textFont);
    root.setProperty("--font-heading", this.stack(headingFont));
    root.setProperty("--font", this.stack(textFont));
    root.setProperty("--heading-weight", this.state.headingWeight);
    root.setProperty("--subheading-weight", this.state.subheadingWeight);
    root.setProperty("--regular", this.state.textWeight);
    root.setProperty("--semibold", this.state.strongWeight);
  }

  stack(name) {
    const font = FONTS.find(([known]) => known === name) ?? FONTS[0];
    return `"${font[0]}", ${font[2]}`;
  }

  // Every icon with a key on the page in the library picked, or the site's own again. An <svg>
  // whose library has no such icon keeps the site's.
  applyIcons() {
    const library = LIBRARIES[this.state.iconLibrary];
    for (const svg of document.querySelectorAll("svg")) {
      const key = keyOf(svg);
      if (!key) continue;
      const name = library && ICONS[key]?.[library.column];
      if (!name) {
        restore(svg);
        continue;
      }
      if (svg.dataset.drawnBy === `${this.state.iconLibrary}:${name}`) continue;
      this.draw(svg, this.state.iconLibrary, library, name);
    }
  }

  async draw(svg, libraryName, library, name) {
    const file = await fetchIcon(library, name);
    // What is picked may have changed while the file came; the next pass draws that.
    if (this.state.iconLibrary !== libraryName) return;
    if (!file) {
      restore(svg);
      return;
    }
    if (!own.has(svg)) {
      own.set(svg, { inner: svg.innerHTML, attributes: Object.fromEntries(ATTRIBUTES.map((at) => [at, svg.getAttribute(at)])) });
    }
    for (const at of ATTRIBUTES) {
      const value = library.attributes?.[at] ?? (at === "viewBox" ? file.viewBox : own.get(svg).attributes[at]);
      if (value === null || value === undefined) svg.removeAttribute(at);
      else svg.setAttribute(at, value);
    }
    for (const property of STYLED) svg.style[property] = library.style?.[property] ?? "";
    svg.innerHTML = file.inner;
    svg.dataset.drawnBy = `${libraryName}:${name}`;
  }

  // A Google font's stylesheet, once; the site's own need none.
  load(name) {
    const font = FONTS.find(([known]) => known === name);
    if (!font?.[1]) return;
    const id = `type-trial-${name.toLowerCase().replaceAll(" ", "-")}`;
    if (document.getElementById(id)) return;
    const link = document.createElement("link");
    link.id = id;
    link.rel = "stylesheet";
    link.href = `https://fonts.googleapis.com/css2?family=${name.replaceAll(" ", "+")}:wght@${font[1]}&display=swap`;
    document.head.append(link);
  }
}
