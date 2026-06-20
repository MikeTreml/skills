import { listItems, runImport, type Item } from "./api";

const searchEl = document.getElementById("search") as HTMLInputElement;
const importBtn = document.getElementById("import") as HTMLButtonElement;
const statusEl = document.getElementById("status")!;
const listEl = document.getElementById("items")!;
const sidebarEl = document.getElementById("sidebar")!;
const emptyEl = document.getElementById("empty") as HTMLParagraphElement;

type Filter = "all" | "skill" | "agent";

let allItems: Item[] = [];
let activeFilter: Filter = "all";
let query = "";

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

function renderSidebar() {
  const c = counts();
  const btn = (key: Filter, label: string, n: number) =>
    `<button class="nav${activeFilter === key ? " active" : ""}" data-filter="${key}">` +
    `<span>${label}</span><span class="count">${n}</span></button>`;
  sidebarEl.innerHTML =
    `<div class="nav-group">${btn("all", "All", c.all)}${btn("skill", "Skills", c.skill)}${btn("agent", "Agents", c.agent)}</div>` +
    `<div class="nav-note">Categories appear here once AI classification (Milestone 3) is built.</div>`;
  for (const b of sidebarEl.querySelectorAll<HTMLButtonElement>(".nav")) {
    b.addEventListener("click", () => {
      activeFilter = b.dataset.filter as Filter;
      renderSidebar();
      renderList();
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
    li.className = "item";
    li.innerHTML =
      `<span class="badge ${it.item_type}">${it.item_type}</span>` +
      `<span class="name">${escapeHtml(it.name)}</span>` +
      (it.has_variants ? `<span class="chip warn">⚠ variants</span>` : "") +
      `<span class="desc">${escapeHtml(it.description)}</span>`;
    frag.appendChild(li);
  }
  listEl.replaceChildren(frag);
  emptyEl.hidden = allItems.length > 0;
  statusEl.textContent = allItems.length
    ? `${items.length} of ${allItems.length} items`
    : "";
}

async function load() {
  allItems = await listItems();
  renderSidebar();
  renderList();
}

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
