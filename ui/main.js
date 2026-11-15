const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const KINDS = [
  ["solid", "Solid"],
  ["dashed", "Dashed"],
  ["dotted", "Dotted"],
  ["double", "Double"],
  ["spacer", "Gap"],
];
const COLORS = ["#8a8a8a", "#ffffff", "#000000", "#4f8cff", "#ff5c5c", "#ffc83d", "#3ecf8e"];

const $ = (id) => document.getElementById(id);
const list = $("list");
const template = $("card");
let items = [];

function toast(msg) {
  const t = $("toast");
  t.textContent = msg;
  t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => (t.hidden = true), 3500);
}

async function call(cmd, args) {
  try {
    const result = await invoke(cmd, args);
    if (Array.isArray(result)) render(result);
    return result;
  } catch (e) {
    toast(String(e));
  }
}

function setImg(img, item) {
  img.src = item.preview1x;
  img.srcset = `${item.preview1x} 1x, ${item.preview2x} 2x`;
}

function renderBar() {
  const bar = $("bar");
  bar.replaceChildren();
  const app = () => Object.assign(document.createElement("div"), { className: "app", innerHTML: "<span></span>" });
  bar.append(app(), app());
  for (const item of items) {
    const s = document.createElement("div");
    s.className = "sep" + (item.live ? "" : " off");
    s.title = item.name;
    const img = document.createElement("img");
    img.alt = "";
    setImg(img, item);
    s.append(img);
    bar.append(s, app());
  }
}

function render(next) {
  const focused = document.activeElement?.closest?.(".card")?.dataset.id;
  items = next;
  $("count").textContent = items.length
    ? `${items.length} separator${items.length === 1 ? "" : "s"} · ${items.filter((i) => i.live).length} live`
    : "No separators yet";
  $("empty").hidden = items.length > 0;
  renderBar();

  // Update cards in place so inputs keep focus while sliders are being dragged.
  const existing = new Map([...list.children].map((c) => [c.dataset.id, c]));
  for (const item of items) {
    const card = existing.get(item.id) ?? buildCard(item);
    existing.delete(item.id);
    fillCard(card, item, item.id === focused);
    list.append(card);
  }
  for (const stale of existing.values()) stale.remove();
}

function buildCard(item) {
  const card = template.content.firstElementChild.cloneNode(true);
  card.dataset.id = item.id;

  const kinds = card.querySelector(".kinds");
  for (const [value, label] of KINDS) {
    const b = document.createElement("button");
    b.type = "button";
    b.role = "radio";
    b.dataset.kind = value;
    b.textContent = label;
    b.onclick = () => save(card, { kind: value });
    kinds.append(b);
  }

  const swatches = card.querySelector(".swatches");
  for (const c of COLORS) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "swatch";
    b.style.background = c;
    b.dataset.color = c;
    b.title = c;
    b.onclick = () => save(card, { color: c });
    swatches.append(b);
  }
  const custom = document.createElement("label");
  custom.className = "swatch custom";
  custom.title = "Custom colour";
  const picker = Object.assign(document.createElement("input"), { type: "color" });
  picker.oninput = () => save(card, { color: picker.value }, true);
  custom.append(picker);
  swatches.append(custom);

  const name = card.querySelector(".name");
  name.onchange = () => save(card, {});
  name.onkeydown = (e) => e.key === "Enter" && name.blur();
  card.querySelector(".thickness").oninput = (e) => save(card, { thickness: +e.target.value }, true);
  card.querySelector(".length").oninput = (e) => save(card, { length: +e.target.value }, true);
  card.querySelector(".live").onchange = (e) => call("set_live", { id: item.id, live: e.target.checked });
  card.querySelector(".remove").onclick = () => {
    const it = items.find((i) => i.id === card.dataset.id);
    call("remove", { id: card.dataset.id });
    toast(it?.live ? "Removed. If you pinned it, also unpin it from the taskbar." : "Removed.");
  };
  return card;
}

function fillCard(card, item, keepName) {
  card.classList.toggle("off", !item.live);
  setImg(card.querySelector(".thumb img"), item);
  const name = card.querySelector(".name");
  if (!keepName || document.activeElement !== name) name.value = item.name;
  card.querySelector(".live").checked = item.live;
  for (const b of card.querySelectorAll(".kinds button")) b.setAttribute("aria-checked", b.dataset.kind === item.style.kind);
  const color = item.style.color.toLowerCase();
  let matched = false;
  for (const b of card.querySelectorAll(".swatch[data-color]")) {
    const on = b.dataset.color === color;
    matched ||= on;
    b.setAttribute("aria-pressed", on);
  }
  const custom = card.querySelector(".swatch.custom");
  custom.setAttribute("aria-pressed", !matched);
  custom.querySelector("input").value = color;
  const spacer = item.style.kind === "spacer";
  for (const el of card.querySelectorAll(".swatches, .slider")) el.style.opacity = spacer ? 0.4 : 1;
  card.querySelector(".thickness").value = item.style.thickness;
  card.querySelector(".length").value = item.style.length;
}

// Slider drags fire many events: coalesce them so each icon file is written once per frame.
const pending = new Map();
function save(card, change, throttle = false) {
  const id = card.dataset.id;
  const item = items.find((i) => i.id === id);
  if (!item) return;
  const prev = pending.get(id)?.style ?? item.style;
  const next = {
    name: card.querySelector(".name").value,
    style: { ...prev, ...change },
  };
  if (!throttle) {
    clearTimeout(pending.get(id)?.timer);
    pending.delete(id);
    return call("update", { id, ...next });
  }
  const p = pending.get(id) ?? {};
  p.style = next.style;
  p.name = next.name;
  clearTimeout(p.timer);
  p.timer = setTimeout(() => {
    pending.delete(id);
    call("update", { id, name: p.name, style: p.style });
  }, 120);
  pending.set(id, p);
}

$("add").onclick = async () => {
  await call("add");
  toast("Added to the end of your taskbar. Drag it where you want it.");
};

$("autostart").onchange = async (e) => {
  const on = await call("set_autostart", { enabled: e.target.checked });
  if (typeof on === "boolean") e.target.checked = on;
};

listen("separators-changed", (e) => render(e.payload));
call("list");
invoke("get_autostart").then((on) => ($("autostart").checked = on));
