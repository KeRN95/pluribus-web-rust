import { Controller } from "@hotwired/stimulus";

// Trying typefaces (TYPE_TRIAL=true in .env; base.html, .type-menu): the top bar's "Aa" panel
// picks the headings' typeface and the text's from the list below, the site's own three and
// Google Fonts', and sets their weights, the headings' (h1) and the subheadings' (h2), the text's
// and its strong's (labels, buttons, card titles). What is picked goes on the page at once, as the
// root's custom properties (base.css reads --font-heading, --font, --heading-weight,
// --subheading-weight, --regular and --semibold), and stays, kept in this browser (localStorage)
// and put back on every page; a Google font's stylesheet is fetched once, as it is picked. "Rikthe"
// puts the site's own back. For the visitor's own browser alone.
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
const DEFAULTS = { headingFont: "Inter", headingWeight: 700, subheadingWeight: 700, textFont: "Geist", textWeight: 400, strongWeight: 600 };

export default class extends Controller {
  static targets = ["field"];

  connect() {
    this.state = { ...DEFAULTS, ...this.saved() };
    for (const select of this.fieldTargets.filter((field) => field.tagName === "SELECT")) {
      select.replaceChildren(...FONTS.map(([name]) => new Option(name, name)));
    }
    this.fill();
    this.apply();
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
    const root = document.documentElement.style;
    const { headingFont, textFont } = this.state;
    const isDefault = Object.entries(DEFAULTS).every(([key, value]) => this.state[key] === value);
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
