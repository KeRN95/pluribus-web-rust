import { Controller } from "@hotwired/stimulus";

// A <select> chosen from a list in the page rather than the phone's own picker (a wheel on
// iPhones, a dialog on Android): a button shows the choice, and a list under it offers the
// options, under their group's heading. The select stays, unseen, and is what the form sends:
// choosing sets it and says so ("change"), so whatever listens to it still hears. Without this
// script the select is just a select.
//
// A list with groups folds, as does a category list (data-picker-tree-value): a group's options, a
// category's subcategories, show under it once its arrow opens them, the path to the choice open
// from the start (towns under their county, models under their make). The posting form's groups,
// "Automjete › Pjesë", nest by their path; the other category lists by the leading
// spaces of category_choices (filters.rs), which give each category's level.
//
// In a card of its own, the row's filter dropdowns (filter_drop.html, data-picker-inline-value),
// the list is the picker: always shown, with no button, named as the select is, reached with Tab
// (its first option, or the choice, taking the focus); choosing sets the select and says so, and
// Escape is the card's, which closes.

let count = 0;
const CHEVRON = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" aria-hidden="true"><g transform="translate(-.75 0) scale(.09375)" stroke="none"><path fill="currentColor" d="M181.66,133.66l-80,80a8,8,0,0,1-11.32-11.32L164.69,128,90.34,53.66a8,8,0,0,1,11.32-11.32l80,80A8,8,0,0,1,181.66,133.66Z"/></g></svg>';

export default class extends Controller {
  static values = { tree: Boolean, placeholder: { type: Boolean, default: true }, inline: Boolean };

  connect() {
    this.select = this.element.querySelector("select");
    if (!this.select) return;
    // A page Turbo kept has the list built already: built again, from the select as it is.
    for (const old of this.element.querySelectorAll("[data-picker-ui]")) old.remove();
    const id = `picker-${++count}`;
    this.tree = this.treeValue || [...this.select.children].some((child) => child.tagName === "OPTGROUP");
    this.label = this.select.labels?.[0];
    if (this.label) this.label.id ||= `${id}-label`;

    this.button = element("button", "picker-button", { type: "button" });
    this.button.setAttribute("aria-haspopup", this.tree ? "tree" : "listbox");
    this.button.setAttribute("aria-expanded", "false");
    this.button.setAttribute("aria-controls", id);
    this.value = element("span", "picker-value", { id: `${id}-value` });
    this.button.append(this.value);
    // Named by the select's label, or by its aria-label ("Monedha"), and then the choice.
    let name = this.label?.id;
    if (!name && this.select.getAttribute("aria-label")) {
      name = `${id}-name`;
      this.button.append(element("span", "visually-hidden", { id: name, textContent: this.select.getAttribute("aria-label") }));
    }
    if (name) this.button.setAttribute("aria-labelledby", `${name} ${id}-value`);

    this.list = element("div", "picker-list", { id, hidden: true, tabIndex: -1 });
    this.list.setAttribute("role", this.tree ? "tree" : "listbox");
    if (name) this.list.setAttribute("aria-labelledby", name);
    for (const part of [this.button, this.list]) part.dataset.pickerUi = "";
    if (this.inlineValue) {
      this.list.hidden = false;
      this.list.tabIndex = 0;
      this.list.classList.add("picker-inline");
      this.list.removeAttribute("aria-labelledby");
      this.list.setAttribute("aria-label", this.select.getAttribute("aria-label") ?? "");
      this.select.after(this.list);
      this.list.addEventListener("focus", (event) => {
        if (this.list.contains(event.relatedTarget)) return;
        const items = this.items();
        (items.find((item) => item.getAttribute("aria-selected") === "true") ?? items[0])?.focus();
      });
    } else {
      this.select.after(this.button, this.list);
    }
    this.select.classList.add("picker-native");
    this.select.tabIndex = -1;
    this.follow();
    if (this.inlineValue) this.fill();
    else this.show();

    this.button.addEventListener("click", () => (this.list.hidden ? this.open() : this.close()));
    this.button.addEventListener("keydown", (event) => {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(event.key)) {
        event.preventDefault();
        this.open();
      }
    });
    this.list.addEventListener("click", (event) => this.clicked(event));
    this.list.addEventListener("keydown", (event) => this.key(event));

    this.shown = () => this.show();
    this.select.addEventListener("change", this.shown);
    // Required and not chosen: the button says so, where the hidden select can't.
    this.invalid = (event) => {
      event.preventDefault();
      this.button.classList.add("picker-invalid");
      this.button.focus();
    };
    this.select.addEventListener("invalid", this.invalid);
    this.outside = (event) => {
      if (!this.element.contains(event.target)) this.close();
    };
    document.addEventListener("click", this.outside);
    // The label would focus the hidden select; it focuses the button instead.
    this.labelled = (event) => {
      event.preventDefault();
      this.button.focus();
    };
    this.label?.addEventListener("click", this.labelled);
    // While the page refreshes in place (refresh.js) the new page's select comes without them: the
    // button and the list stay, and the select hidden, and the button then shows what the new page
    // chose.
    this.keepUi = (event) => {
      if (event.target.hasAttribute?.("data-picker-ui")) event.preventDefault();
    };
    this.keepHidden = (event) => {
      if (event.target === this.select && ["class", "tabindex"].includes(event.detail.attributeName)) event.preventDefault();
    };
    this.morphed = (event) => {
      if (event.target === this.select) this.show();
    };
    this.element.addEventListener("turbo:before-morph-element", this.keepUi);
    this.element.addEventListener("turbo:before-morph-attribute", this.keepHidden);
    this.element.addEventListener("turbo:morph-element", this.morphed);
  }

  disconnect() {
    if (!this.select) return;
    document.removeEventListener("click", this.outside);
    this.select.removeEventListener("change", this.shown);
    this.select.removeEventListener("invalid", this.invalid);
    this.label?.removeEventListener("click", this.labelled);
    this.element.removeEventListener("turbo:before-morph-element", this.keepUi);
    this.element.removeEventListener("turbo:before-morph-attribute", this.keepHidden);
    this.element.removeEventListener("turbo:morph-element", this.morphed);
    delete this.select.value;
    delete this.select.selectedIndex;
  }

  // Scripts that set the select (listing_form_controller.js: the kind of listing a category takes,
  // its currency, a model that no longer fits its make) don't say so with an event: the button
  // follows the value however it is set.
  follow() {
    const picker = this;
    for (const property of ["value", "selectedIndex"]) {
      const own = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, property);
      Object.defineProperty(this.select, property, {
        configurable: true,
        get() {
          return own.get.call(this);
        },
        set(value) {
          own.set.call(this, value);
          picker.show();
        },
      });
    }
  }

  // The button, and the list if it is open, show what the select has.
  show() {
    const chosen = this.select.selectedOptions[0];
    this.value.textContent = chosen ? chosen.textContent.trim() : "";
    // Nothing chosen yet: where the empty choice names the field (the posting form), it looks
    // like a placeholder; not where it is a choice itself ("Posto si: Unë",
    // data-picker-placeholder-value="false").
    this.button.classList.toggle("is-empty", this.placeholderValue && !this.select.value);
    for (const item of this.list.querySelectorAll("[data-value]")) {
      item.setAttribute("aria-selected", String(item.dataset.value === this.select.value));
    }
    if (this.select.value) this.button.classList.remove("picker-invalid");
  }

  open() {
    // Built each time, from the options as they are now: a script may have hidden some.
    this.fill();
    // Wider than a narrow field, the list grows from the field's side nearer the middle of the
    // screen: rightward from the welcome form's town, leftward from its category.
    const field = this.element.getBoundingClientRect();
    this.list.classList.toggle("picker-start", field.left + field.width / 2 < document.documentElement.clientWidth / 2);
    this.list.hidden = false;
    this.button.setAttribute("aria-expanded", "true");
    // The choice in view and in focus, ready for the arrow keys; the list in view on the page.
    const items = this.items();
    const current = items.find((item) => item.getAttribute("aria-selected") === "true") ?? items[0];
    if (current) {
      this.list.scrollTop = current.offsetTop - this.list.clientHeight / 2;
      current.focus({ preventScroll: true });
    }
    this.list.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }

  close(refocus = false) {
    if (this.list.hidden || this.inlineValue) return;
    this.list.hidden = true;
    this.button.setAttribute("aria-expanded", "false");
    if (refocus) this.button.focus();
  }

  choose(item) {
    const changed = this.select.value !== item.dataset.value;
    this.select.value = item.dataset.value;
    this.close(true);
    if (changed) this.select.dispatchEvent(new Event("change", { bubbles: true }));
  }

  // The options a person can pick now: not a required select's "Zgjidh…", nor hidden ones.
  shownOptions(options) {
    return [...options].filter((option) => !option.hidden && !option.disabled && !(option.value === "" && this.select.required));
  }

  fill() {
    this.list.replaceChildren();
    if (this.tree) this.fillTree();
    else this.fillList();
    this.show();
  }

  // As the select has them, each group under its heading.
  fillList() {
    for (const child of this.select.children) {
      if (child.tagName !== "OPTGROUP") {
        for (const option of this.shownOptions([child])) this.list.append(this.optionItem(option, 0));
        continue;
      }
      const options = this.shownOptions(child.children);
      if (child.hidden || !options.length) continue;
      const group = element("div", "picker-group");
      group.setAttribute("role", "group");
      group.setAttribute("aria-label", child.label);
      const heading = element("div", "picker-heading", { textContent: child.label });
      heading.setAttribute("aria-hidden", "true");
      group.append(heading, ...options.map((option) => this.optionItem(option, 0)));
      this.list.append(group);
    }
  }

  // Categories as a tree: [{ label, option, children }], each subcategory under its category.
  fillTree() {
    const root = { children: [] };
    const leaf = (option) => ({ key: option.value, label: option.textContent.trim(), option, children: [] });
    if ([...this.select.children].some((child) => child.tagName === "OPTGROUP")) {
      // The posting form: groups by path, "Automjete › Pjesë" under "Automjete".
      const groups = new Map();
      const node = (path) => {
        if (!groups.has(path)) {
          const parts = path.split(" › ");
          const parent = parts.length > 1 ? node(parts.slice(0, -1).join(" › ")) : root;
          const made = { key: `group:${path}`, label: parts.at(-1), option: null, children: [] };
          parent.children.push(made);
          groups.set(path, made);
        }
        return groups.get(path);
      };
      for (const child of this.select.children) {
        if (child.tagName === "OPTGROUP") {
          if (!child.hidden) node(child.label).children.push(...this.shownOptions(child.children).map(leaf));
        } else {
          root.children.push(...this.shownOptions([child]).map(leaf));
        }
      }
    } else {
      // Levels by leading spaces: a category's subcategories follow it, a level further in.
      const path = [root];
      for (const option of this.shownOptions(this.select.options)) {
        const level = Math.floor(option.textContent.match(/^ */)[0].length / 3);
        path.length = Math.min(path.length, level + 1);
        const made = leaf(option);
        path.at(-1).children.push(made);
        path.push(made);
      }
    }
    // Open from the start: the categories on the way to the choice.
    if (!this.opened) {
      this.opened = new Set();
      const find = (node, trail) => {
        for (const child of node.children) {
          if (child.option?.value === this.select.value && child.option.value !== "") {
            for (const key of trail) this.opened.add(key);
          }
          find(child, [...trail, child.key]);
        }
      };
      find(root, []);
    }
    // A list of one group comes open, the models of the make chosen say: nothing to choose between.
    const branches = root.children.filter((child) => child.children.length);
    if (branches.length === 1) this.opened.add(branches[0].key);
    this.branch(root, this.list, 1);
  }

  branch(node, container, level) {
    for (const child of node.children) {
      if (!child.children.length) {
        if (child.option) container.append(this.optionItem(child.option, level));
        continue;
      }
      const open = this.opened.has(child.key);
      const row = child.option ? this.optionItem(child.option, level) : element("div", "picker-option", { textContent: child.label, tabIndex: -1 });
      row.setAttribute("role", "treeitem");
      row.setAttribute("aria-level", level);
      row.setAttribute("aria-expanded", String(open));
      row.classList.add("picker-branch");
      row.dataset.key = child.key;
      const toggle = element("span", "picker-toggle", { innerHTML: CHEVRON });
      toggle.setAttribute("aria-hidden", "true");
      row.append(toggle);
      const children = element("div", "picker-children", { hidden: !open });
      children.setAttribute("role", "group");
      container.append(row, children);
      this.branch(child, children, level + 1);
    }
  }

  optionItem(option, level) {
    const item = element("div", "picker-option", { textContent: option.textContent.trim(), tabIndex: -1 });
    item.setAttribute("role", this.tree ? "treeitem" : "option");
    item.dataset.value = option.value;
    if (this.tree) item.setAttribute("aria-level", level);
    return item;
  }

  // Opens or closes a category's subcategories.
  fold(row, open = row.getAttribute("aria-expanded") !== "true") {
    row.setAttribute("aria-expanded", String(open));
    row.nextElementSibling.hidden = !open;
    if (open) this.opened.add(row.dataset.key);
    else this.opened.delete(row.dataset.key);
  }

  clicked(event) {
    const item = event.target.closest(".picker-option");
    if (!item) return;
    // The arrow, or a heading that can't be chosen itself, folds; the rest is chosen.
    if (item.classList.contains("picker-branch") && (event.target.closest(".picker-toggle") || !("value" in item.dataset))) {
      this.fold(item);
      item.focus({ preventScroll: true });
    } else if ("value" in item.dataset) {
      this.choose(item);
    }
  }

  // What the arrow keys move through: the options in view, folded ones aside.
  items() {
    return [...this.list.querySelectorAll(".picker-option")].filter((item) => item.offsetParent !== null);
  }

  key(event) {
    const items = this.items();
    const at = items.indexOf(document.activeElement);
    const item = items[at];
    const go = (index) => items[Math.max(0, Math.min(items.length - 1, index))]?.focus();
    const branch = item?.classList.contains("picker-branch");
    const open = item?.getAttribute("aria-expanded") === "true";
    switch (event.key) {
      case "ArrowDown": go(at + 1); break;
      case "ArrowUp": go(at - 1); break;
      case "Home": go(0); break;
      case "End": go(items.length - 1); break;
      case "ArrowRight":
        if (branch && !open) this.fold(item, true);
        else if (branch) go(at + 1);
        break;
      case "ArrowLeft":
        if (branch && open) this.fold(item, false);
        else item?.parentElement.closest(".picker-children")?.previousElementSibling?.focus();
        break;
      case "Enter":
      case " ":
        if (item && "value" in item.dataset) this.choose(item);
        else if (branch) this.fold(item);
        break;
      case "Escape":
        // Inline, the card it is in closes (dropdown_controller.js).
        if (this.inlineValue) return;
        // The list closes, not what holds it too (the filter sheet closes on Escape as well).
        event.stopPropagation();
        this.close(true);
        break;
      case "Tab": this.close(); return;
      default: {
        // A letter: the next option in view that starts with it.
        if (event.key.length !== 1 || !/\S/.test(event.key)) return;
        const letter = event.key.toLocaleLowerCase("sq");
        const after = [...items.slice(at + 1), ...items.slice(0, at + 1)];
        after.find((other) => other.textContent.toLocaleLowerCase("sq").startsWith(letter))?.focus();
      }
    }
    event.preventDefault();
  }
}

function element(tag, className, properties = {}) {
  return Object.assign(document.createElement(tag), { className, ...properties });
}
