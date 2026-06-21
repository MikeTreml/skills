import {
  addScanDir,
  addSynonym,
  aiAvailable,
  classifyAll,
  getItemContent,
  listDuplicates,
  listItems,
  listScanDirs,
  listVerbMap,
  removeScanDir,
  removeSynonym,
  runImport,
  type DupGroup,
  type Item,
  type ItemType,
  type ScanDir,
} from "./api";

const searchEl = document.getElementById("search") as HTMLInputElement;
const importBtn = document.getElementById("import") as HTMLButtonElement;
const classifyBtn = document.getElementById("classify") as HTMLButtonElement;
const statusEl = document.getElementById("status")!;
const listEl = document.getElementById("items")!;
const dupesEl = document.getElementById("dupes")!;
const filtersEl = document.getElementById("filters")!;
const sourcesEl = document.getElementById("sources")!;
const verbmapEl = document.getElementById("verbmap")!;
const emptyEl = document.getElementById("empty") as HTMLParagraphElement;
const detailEl = document.getElementById("detail") as HTMLElement;

type TypeFilter = "all" | "skill" | "agent";

let allItems: Item[] = [];
let scanDirs: ScanDir[] = [];
let dupGroups: DupGroup[] = [];
let verbMap: [string, string][] = [];
let aiOk = false;
let view: "library" | "duplicates" = "library";
let typeFilter: TypeFilter = "all";
let objectFilter: string | null = null; // null=all, "__none__"=untriaged
let query = "";
let selectedId: number | null = null;

const esc = (s: string) =>
  s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const itemById = (id: number) => allItems.find((i) => i.id === id);

// ---------- sidebar: type filter + object tree + view toggle ----------
function renderFilters() {
  const typeCount = (t: TypeFilter) =>
    t === "all" ? allItems.length : allItems.filter((i) => i.item_type === t).length;
  const typeBtn = (t: TypeFilter, label: string) =>
    `<button class="nav${typeFilter === t ? " active" : ""}" data-type="${t}">` +
    `<span>${label}</span><span class="count">${typeCount(t)}</span></button>`;

  const objects = new Map<string, number>();
  let untriaged = 0;
  for (const it of allItems) {
    if (it.object) objects.set(it.object, (objects.get(it.object) ?? 0) + 1);
    else untriaged++;
  }
  const sortedObjects = [...objects.entries()].sort((a, b) => b[1] - a[1]);
  const objBtn = (key: string | null, label: string, n: number) =>
    `<button class="nav sub${objectFilter === key && view === "library" ? " active" : ""}" data-object="${key ?? ""}">` +
    `<span>${esc(label)}</span><span class="count">${n}</span></button>`;

  let objectTree = "";
  if (sortedObjects.length || untriaged) {
    objectTree =
      `<div class="nav-head">Objects</div>` +
      objBtn(null, "All objects", allItems.length) +
      sortedObjects.map(([o, n]) => objBtn(o, o, n)).join("") +
      (untriaged ? objBtn("__none__", "Untriaged", untriaged) : "");
  } else {
    objectTree = `<div class="nav-note">Run <b>Classify</b> to group by Object.</div>`;
  }

  filtersEl.innerHTML =
    `<div class="nav-group">${typeBtn("all", "All")}${typeBtn("skill", "Skills")}${typeBtn("agent", "Agents")}</div>` +
    `<div class="nav-group">` +
    `<button class="nav${view === "library" ? " active" : ""}" data-view="library"><span>Library</span></button>` +
    `<button class="nav${view === "duplicates" ? " active" : ""}" data-view="duplicates"><span>Duplicates</span><span class="count">${dupGroups.length}</span></button>` +
    `</div>` +
    `<div class="nav-group">${objectTree}</div>`;

  for (const b of filtersEl.querySelectorAll<HTMLButtonElement>("[data-type]"))
    b.addEventListener("click", () => {
      typeFilter = b.dataset.type as TypeFilter;
      renderFilters();
      renderMain();
    });
  for (const b of filtersEl.querySelectorAll<HTMLButtonElement>("[data-view]"))
    b.addEventListener("click", () => {
      view = b.dataset.view as "library" | "duplicates";
      renderFilters();
      renderMain();
    });
  for (const b of filtersEl.querySelectorAll<HTMLButtonElement>("[data-object]"))
    b.addEventListener("click", () => {
      objectFilter = b.dataset.object === "" ? null : b.dataset.object!;
      view = "library";
      renderFilters();
      renderMain();
    });
}

// ---------- sidebar: custom sources ----------
function renderSources() {
  const rows = scanDirs
    .map(
      (d) =>
        `<li class="src-item"><span class="badge ${d.item_type}">${d.item_type}</span>` +
        `<span class="src-path" title="${esc(d.path)}">${esc(d.path)}</span>` +
        `<button class="src-rm" data-id="${d.id}" title="Remove">✕</button></li>`,
    )
    .join("");
  sourcesEl.innerHTML =
    `<h3>Custom sources</h3>` +
    `<input id="dir-input" class="dir-input" placeholder="C:\\path\\to\\folder" />` +
    `<div class="add-row"><button id="add-agents" class="add-btn">+ Agents dir</button>` +
    `<button id="add-skills" class="add-btn">+ Skills dir</button></div>` +
    `<ul class="src-list">${rows}</ul>`;

  const input = document.getElementById("dir-input") as HTMLInputElement;
  const add = async (t: ItemType) => {
    const path = input.value.trim();
    if (!path) return;
    try {
      await addScanDir(path, t);
      input.value = "";
      scanDirs = await listScanDirs();
      renderSources();
      statusEl.textContent = `Added ${t} source — click Scan & import`;
    } catch (e) {
      statusEl.textContent = `Error: ${e}`;
    }
  };
  document.getElementById("add-agents")!.addEventListener("click", () => add("agent"));
  document.getElementById("add-skills")!.addEventListener("click", () => add("skill"));
  for (const b of sourcesEl.querySelectorAll<HTMLButtonElement>(".src-rm"))
    b.addEventListener("click", async () => {
      await removeScanDir(Number(b.dataset.id));
      scanDirs = await listScanDirs();
      renderSources();
    });
}

// ---------- sidebar: verb map editor ----------
function renderVerbMap() {
  const byCanon = new Map<string, string[]>();
  for (const [canon, syn] of verbMap) {
    if (!byCanon.has(canon)) byCanon.set(canon, []);
    byCanon.get(canon)!.push(syn);
  }
  const rows = [...byCanon.entries()]
    .sort()
    .map(
      ([canon, syns]) =>
        `<div class="verb-row"><b>${esc(canon)}</b> ` +
        syns
          .map((s) => `<span class="vchip">${esc(s)}<button class="vrm" data-syn="${esc(s)}">✕</button></span>`)
          .join(" ") +
        `</div>`,
    )
    .join("");
  verbmapEl.innerHTML =
    `<details><summary>Verb map (${verbMap.length})</summary>` +
    `<div class="verb-list">${rows}</div>` +
    `<div class="add-row"><input id="vc" class="dir-input" placeholder="Canonical" />` +
    `<input id="vs" class="dir-input" placeholder="synonym" /></div>` +
    `<div class="add-row"><button id="vadd" class="add-btn">+ Add synonym</button></div>` +
    `</details>`;

  document.getElementById("vadd")!.addEventListener("click", async () => {
    const c = (document.getElementById("vc") as HTMLInputElement).value.trim();
    const s = (document.getElementById("vs") as HTMLInputElement).value.trim();
    if (!c || !s) return;
    await addSynonym(c, s);
    verbMap = await listVerbMap();
    renderVerbMap();
  });
  for (const b of verbmapEl.querySelectorAll<HTMLButtonElement>(".vrm"))
    b.addEventListener("click", async () => {
      await removeSynonym(b.dataset.syn!);
      verbMap = await listVerbMap();
      renderVerbMap();
    });
}

// ---------- main content ----------
function chips(it: Item): string {
  const c: string[] = [];
  if (it.object) c.push(`<span class="chip obj">${esc(it.object)}${it.sub_object ? " › " + esc(it.sub_object) : ""}</span>`);
  if (it.verb) c.push(`<span class="chip verb">${esc(it.verb)}</span>`);
  if (it.qualifier) c.push(`<span class="chip qual">${esc(it.qualifier)}</span>`);
  if (it.has_variants) c.push(`<span class="chip warn">⚠ variants</span>`);
  return c.join("");
}

function itemRow(it: Item): string {
  return (
    `<li class="item${it.id === selectedId ? " active" : ""}" data-id="${it.id}">` +
    `<span class="badge ${it.item_type}">${it.item_type}</span>` +
    `<span class="name">${esc(it.name)}</span>${chips(it)}` +
    `<span class="desc">${esc(it.description)}</span></li>`
  );
}

function visibleItems(): Item[] {
  const q = query.trim().toLowerCase();
  return allItems.filter((it) => {
    if (typeFilter !== "all" && it.item_type !== typeFilter) return false;
    if (objectFilter === "__none__" && it.object) return false;
    if (objectFilter && objectFilter !== "__none__" && it.object !== objectFilter) return false;
    if (!q) return true;
    return it.name.toLowerCase().includes(q) || it.description.toLowerCase().includes(q);
  });
}

function renderList() {
  const items = visibleItems();
  listEl.innerHTML = items.map(itemRow).join("");
  emptyEl.hidden = allItems.length > 0;
  statusEl.textContent = allItems.length ? `${items.length} of ${allItems.length} items` : "";
}

function renderDuplicates() {
  if (!dupGroups.length) {
    dupesEl.innerHTML = `<p class="empty">No duplicates yet — run <b>Classify</b> first.</p>`;
    return;
  }
  const order = { exact: 0, near: 1 } as const;
  const groups = [...dupGroups].sort((a, b) => order[a.kind] - order[b.kind]);
  dupesEl.innerHTML = groups
    .map((g) => {
      const members = g.item_ids
        .map(itemById)
        .filter((x): x is Item => !!x)
        .map(itemRow)
        .join("");
      return (
        `<div class="dup-group"><div class="dup-head"><span class="chip ${g.kind === "exact" ? "warn" : "verb"}">${g.kind}</span> ` +
        `<b>${esc(g.key)}</b> <span class="count">${g.item_ids.length}</span></div>` +
        `<ul class="items">${members}</ul></div>`
      );
    })
    .join("");
  statusEl.textContent = `${dupGroups.length} duplicate/similar groups`;
}

function renderMain() {
  if (view === "duplicates") {
    listEl.hidden = true;
    dupesEl.hidden = false;
    renderDuplicates();
  } else {
    dupesEl.hidden = true;
    listEl.hidden = false;
    renderList();
  }
}

// ---------- detail / preview ----------
function closeDetail() {
  selectedId = null;
  detailEl.hidden = true;
  detailEl.innerHTML = "";
  renderMain();
}

async function openDetail(id: number) {
  const it = itemById(id);
  if (!it) return;
  selectedId = id;
  renderMain();
  detailEl.hidden = false;
  detailEl.innerHTML =
    `<div class="detail-head"><div class="detail-title">` +
    `<span class="badge ${it.item_type}">${it.item_type}</span><b>${esc(it.name)}</b></div>` +
    `<button id="detail-close" class="src-rm" title="Close">✕</button></div>` +
    `<div class="detail-chips">${chips(it)}</div>` +
    (it.description ? `<p class="detail-desc">${esc(it.description)}</p>` : "") +
    `<div class="detail-path" title="${esc(it.library_path)}">${esc(it.library_path)}</div>` +
    `<pre class="detail-body">Loading…</pre>`;
  document.getElementById("detail-close")!.addEventListener("click", closeDetail);
  const body = detailEl.querySelector(".detail-body")!;
  try {
    body.textContent = await getItemContent(id);
  } catch (e) {
    body.textContent = `Error: ${e}`;
  }
}

// ---------- load + events ----------
async function load() {
  const [items, dirs, ok, vmap, dups] = await Promise.all([
    listItems(),
    listScanDirs(),
    aiAvailable(),
    listVerbMap(),
    listDuplicates(),
  ]);
  allItems = items;
  scanDirs = dirs;
  aiOk = ok;
  verbMap = vmap;
  dupGroups = dups;
  classifyBtn.disabled = !aiOk;
  classifyBtn.title = aiOk ? "Classify with AI" : "Set OPENAI_API_KEY to enable";
  renderFilters();
  renderSources();
  renderVerbMap();
  renderMain();
}

listEl.addEventListener("click", (e) => {
  const li = (e.target as HTMLElement).closest("li.item") as HTMLElement | null;
  if (li?.dataset.id) openDetail(Number(li.dataset.id));
});
dupesEl.addEventListener("click", (e) => {
  const li = (e.target as HTMLElement).closest("li.item") as HTMLElement | null;
  if (li?.dataset.id) openDetail(Number(li.dataset.id));
});
searchEl.addEventListener("input", () => {
  query = searchEl.value;
  if (view !== "library") view = "library";
  renderMain();
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

classifyBtn.addEventListener("click", async () => {
  if (!aiOk) {
    statusEl.textContent = "Set a valid OPENAI_API_KEY, then restart, to classify.";
    return;
  }
  classifyBtn.disabled = true;
  statusEl.textContent = "Classifying… (one cheap call per ~20 items)";
  try {
    const s = await classifyAll();
    await load();
    statusEl.textContent = `Classified ${s.classified} of ${s.total}`;
  } catch (e) {
    statusEl.textContent = `Error: ${e}`;
  } finally {
    classifyBtn.disabled = false;
  }
});

load().catch((e) => (statusEl.textContent = `Error: ${e}`));
