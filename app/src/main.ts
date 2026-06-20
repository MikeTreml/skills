import {
  addScanDir,
  getItemContent,
  listItems,
  listScanDirs,
  removeScanDir,
  runImport,
  type Item,
  type ItemType,
  type ScanDir,
} from "./api";

const searchEl = document.getElementById("search") as HTMLInputElement;
const importBtn = document.getElementById("import") as HTMLButtonElement;
const statusEl = document.getElementById("status")!;
const listEl = document.getElementById("items")!;
const filtersEl = document.getElementById("filters")!;
const sourcesEl = document.getElementById("sources")!;
const emptyEl = document.getElementById("empty") as HTMLParagraphElement;
const detailEl = document.getElementById("detail") as HTMLElement;

type Filter = "all" | "skill" | "agent";

let allItems: Item[] = [];
let scanDirs: ScanDir[] = [];
let activeFilter: Filter = "all";
let query = "";
let selectedId: number | null = null;

function escapeHtml(s: string): string {
  const map: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" };
  return s.replace(/[&<>"]/g, (c) => map[c]);
}

function counts() {
  let skill = 0;
  let agent = 0;
  for (const it of allItems) it.item_type === "agent" ? agent++ : skill++;
  return { all: allItems.length, skill, agent };
}

function renderFilters() {
  const c = counts();
  const btn = (key: Filter, label: string, n: number) =>
    `<button class="nav${activeFilter === key ? " active" : ""}" data-filter="${key}">` +
    `<span>${label}</span><span class="count">${n}</span></button>`;
  filtersEl.innerHTML = `<div class="nav-group">${btn("all", "All", c.all)}${btn(
    "skill",
    "Skills",
    c.skill,
  )}${btn("agent", "Agents", c.agent)}</div>`;
  for (const b of filtersEl.querySelectorAll<HTMLButtonElement>(".nav")) {
    b.addEventListener("click", () => {
      activeFilter = b.dataset.filter as Filter;
      renderFilters();
      renderList();
    });
  }
}

function renderSources() {
  const rows = scanDirs
    .map(
      (d) =>
        `<li class="src-item"><span class="badge ${d.item_type}">${d.item_type}</span>` +
        `<span class="src-path" title="${escapeHtml(d.path)}">${escapeHtml(d.path)}</span>` +
        `<button class="src-rm" data-id="${d.id}" title="Remove">✕</button></li>`,
    )
    .join("");
  sourcesEl.innerHTML =
    `<h3>Custom sources</h3>` +
    `<input id="dir-input" class="dir-input" placeholder="C:\\path\\to\\folder" />` +
    `<div class="add-row">` +
    `<button id="add-agents" class="add-btn">+ Agents dir</button>` +
    `<button id="add-skills" class="add-btn">+ Skills dir</button>` +
    `</div>` +
    `<ul class="src-list">${rows}</ul>` +
    (scanDirs.length ? `<div class="nav-note">Click <b>Scan &amp; import</b> to pick up changes.</div>` : "");

  const input = document.getElementById("dir-input") as HTMLInputElement;
  const add = async (type: ItemType) => {
    const path = input.value.trim();
    if (!path) return;
    try {
      await addScanDir(path, type);
      input.value = "";
      scanDirs = await listScanDirs();
      renderSources();
      statusEl.textContent = `Added ${type} source — click Scan & import`;
    } catch (e) {
      statusEl.textContent = `Error: ${e}`;
    }
  };
  document.getElementById("add-agents")!.addEventListener("click", () => add("agent"));
  document.getElementById("add-skills")!.addEventListener("click", () => add("skill"));
  for (const b of sourcesEl.querySelectorAll<HTMLButtonElement>(".src-rm")) {
    b.addEventListener("click", async () => {
      await removeScanDir(Number(b.dataset.id));
      scanDirs = await listScanDirs();
      renderSources();
    });
  }
}

function visibleItems(): Item[] {
  const q = query.trim().toLowerCase();
  return allItems.filter((it) => {
    if (activeFilter !== "all" && it.item_type !== activeFilter) return false;
    if (!q) return true;
    return it.name.toLowerCase().includes(q) || it.description.toLowerCase().includes(q);
  });
}

function renderList() {
  const items = visibleItems();
  const frag = document.createDocumentFragment();
  for (const it of items) {
    const li = document.createElement("li");
    li.className = it.id === selectedId ? "item active" : "item";
    li.dataset.id = String(it.id);
    li.innerHTML =
      `<span class="badge ${it.item_type}">${it.item_type}</span>` +
      `<span class="name">${escapeHtml(it.name)}</span>` +
      (it.has_variants ? `<span class="chip warn">⚠ variants</span>` : "") +
      `<span class="desc">${escapeHtml(it.description)}</span>`;
    frag.appendChild(li);
  }
  listEl.replaceChildren(frag);
  emptyEl.hidden = allItems.length > 0;
  statusEl.textContent = allItems.length ? `${items.length} of ${allItems.length} items` : "";
}

async function load() {
  [allItems, scanDirs] = await Promise.all([listItems(), listScanDirs()]);
  renderFilters();
  renderSources();
  renderList();
}

function closeDetail() {
  selectedId = null;
  detailEl.hidden = true;
  detailEl.innerHTML = "";
  renderList();
}

async function openDetail(id: number) {
  const it = allItems.find((i) => i.id === id);
  if (!it) return;
  selectedId = id;
  renderList();
  detailEl.hidden = false;
  detailEl.innerHTML =
    `<div class="detail-head">` +
    `<div class="detail-title"><span class="badge ${it.item_type}">${it.item_type}</span>` +
    `<b>${escapeHtml(it.name)}</b>${it.has_variants ? ` <span class="chip warn">⚠ variants</span>` : ""}</div>` +
    `<button id="detail-close" class="src-rm" title="Close">✕</button></div>` +
    (it.description ? `<p class="detail-desc">${escapeHtml(it.description)}</p>` : "") +
    `<div class="detail-path" title="${escapeHtml(it.library_path)}">${escapeHtml(it.library_path)}</div>` +
    `<pre class="detail-body">Loading…</pre>`;
  document.getElementById("detail-close")!.addEventListener("click", closeDetail);
  const body = detailEl.querySelector(".detail-body")!;
  try {
    body.textContent = await getItemContent(id);
  } catch (e) {
    body.textContent = `Error: ${e}`;
  }
}

listEl.addEventListener("click", (e) => {
  const li = (e.target as HTMLElement).closest("li.item") as HTMLElement | null;
  if (li?.dataset.id) openDetail(Number(li.dataset.id));
});

searchEl.addEventListener("input", () => {
  query = searchEl.value;
  renderList();
});

importBtn.addEventListener("click", async () => {
  importBtn.disabled = true;
  statusEl.textContent = "Importing… (scanning locations + tarball)";
  try {
    const s = await runImport();
    await load();
    statusEl.textContent = `Imported ${s.items_new} new · ${s.variants_flagged} variants · ${allItems.length} total`;
  } catch (e) {
    statusEl.textContent = `Error: ${e}`;
  } finally {
    importBtn.disabled = false;
  }
});

load().catch((e) => (statusEl.textContent = `Error: ${e}`));
