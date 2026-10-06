import { Controller } from "@hotwired/stimulus";

// A <select> chosen from a list in the page rather than the phone's own picker (a wheel on
// iPhones, a dialog on Android): a button shows the choice, and a list under it offers the
// options, under their group's heading ("Automjete › Pjesë dhe aksesorë") or indented by their
// level (the leading spaces category_choices gives them, filters.rs). The select stays, unseen,
// and is what the form sends: choosing sets it and says so ("change"), so whatever listens to it
// still hears. Without this script the select is just a select.

let count = 0;

export default class extends Controller {
  connect() {
    this.select = this.element.querySelector("select");
    if (!this.select) return;
    // A page Turbo kept has the list built already: built again, from the select as it is.
    for (const old of this.element.querySelectorAll("[data-picker-ui]")) old.remove();
    const id = `picker-${++count}`;
    this.label = this.select.labels?.[0];
    if (this.label) this.label.id ||= `${id}-label`;

    this.button = element("button", "picker-button", { type: "button" });
    this.button.setAttribute("aria-haspopup", "listbox");
    this.button.setAttribute("aria-expanded", "false");
    this.button.setAttribute("aria-controls", id);
    this.value = element("span", "picker-value", { id: `${id}-value` });
    this.button.append(this.value);
    if (this.label) this.button.setAttribute("aria-labelledby", `${this.label.id} ${id}-value`);

    this.list = element("div", "picker-list", { id, hidden: true, tabIndex: -1 });
    this.list.setAttribute("role", "listbox");
    if (this.label) this.list.setAttribute("aria-labelledby", this.label.id);
    this.fill();
    for (const part of [this.button, this.list]) part.dataset.pickerUi = "";
    this.select.after(this.button, this.list);
    this.select.classList.add("picker-native");
    this.select.tabIndex = -1;
    this.show();

    this.button.addEventListener("click", () => (this.list.hidden ? this.open() : this.close()));
    this.button.addEventListener("keydown", (event) => {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(event.key)) {
        event.preventDefault();
        this.open();
      }
    });
    this.list.addEventListener("click", (event) => {
      const option = event.target.closest("[role=option]");
      if (option) this.choose(option);
    });
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
  }

  disconnect() {
    if (!this.select) return;
    document.removeEventListener("click", this.outside);
    this.select.removeEventListener("change", this.shown);
    this.select.removeEventListener("invalid", this.invalid);
    this.label?.removeEventListener("click", this.labelled);
  }

  // The options as the select has them: each group under its heading; without groups, by level.
  fill() {
    const add = (option, parent) => {
      // A required select's "Zgjidh…" is only what the button says while nothing is chosen.
      if (option.value === "" && this.select.required) return;
      const indent = option.textContent.match(/^ */)[0].length;
      const item = element("div", "picker-option", { textContent: option.textContent.trim(), tabIndex: -1 });
      item.setAttribute("role", "option");
      item.dataset.value = option.value;
      item.dataset.depth = Math.floor(indent / 3);
      parent.append(item);
    };
    const groups = [...this.select.children].some((child) => child.tagName === "OPTGROUP");
    this.list.classList.toggle("picker-levels", !groups);
    for (const child of this.select.children) {
      if (child.tagName !== "OPTGROUP") {
        add(child, this.list);
        continue;
      }
      const group = element("div", "picker-group");
      group.setAttribute("role", "group");
      group.setAttribute("aria-label", child.label);
      const heading = element("div", "picker-heading", { textContent: child.label });
      heading.setAttribute("aria-hidden", "true");
      group.append(heading);
      for (const option of child.children) add(option, group);
      this.list.append(group);
    }
  }

  get options() {
    return [...this.list.querySelectorAll("[role=option]")];
  }

  // The button and the list follow the select.
  show() {
    const chosen = this.select.selectedOptions[0];
    this.value.textContent = chosen ? chosen.textContent.trim() : "";
    for (const item of this.options) item.setAttribute("aria-selected", String(item.dataset.value === this.select.value));
    if (this.select.value) this.button.classList.remove("picker-invalid");
  }

  open() {
    this.list.hidden = false;
    this.button.setAttribute("aria-expanded", "true");
    // The choice in view and in focus, ready for the arrow keys; the list in view on the page.
    const current = this.options.find((item) => item.getAttribute("aria-selected") === "true") ?? this.options[0];
    if (current) {
      this.list.scrollTop = current.offsetTop - this.list.clientHeight / 2;
      current.focus({ preventScroll: true });
    }
    this.list.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }

  close(refocus = false) {
    if (this.list.hidden) return;
    this.list.hidden = true;
    this.button.setAttribute("aria-expanded", "false");
    if (refocus) this.button.focus();
  }

  choose(item) {
    const changed = this.select.value !== item.dataset.value;
    this.select.value = item.dataset.value;
    this.show();
    this.close(true);
    if (changed) this.select.dispatchEvent(new Event("change", { bubbles: true }));
  }

  key(event) {
    const options = this.options;
    const at = options.indexOf(document.activeElement);
    const go = (index) => options[Math.max(0, Math.min(options.length - 1, index))]?.focus();
    switch (event.key) {
      case "ArrowDown": go(at + 1); break;
      case "ArrowUp": go(at - 1); break;
      case "Home": go(0); break;
      case "End": go(options.length - 1); break;
      case "Enter":
      case " ":
        if (at >= 0) this.choose(options[at]);
        break;
      case "Escape":
        // The list closes, not what holds it too (the filter sheet closes on Escape as well).
        event.stopPropagation();
        this.close(true);
        break;
      case "Tab": this.close(); return;
      default: {
        // A letter: the next option that starts with it.
        if (event.key.length !== 1 || !/\S/.test(event.key)) return;
        const letter = event.key.toLocaleLowerCase("sq");
        const after = [...options.slice(at + 1), ...options.slice(0, at + 1)];
        after.find((item) => item.textContent.toLocaleLowerCase("sq").startsWith(letter))?.focus();
      }
    }
    event.preventDefault();
  }
}

function element(tag, className, properties = {}) {
  return Object.assign(document.createElement(tag), { className, ...properties });
}
