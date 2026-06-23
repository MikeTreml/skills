import {
  addScanDir,
  addSynonym,
  aiAvailable,
  applyRefinement,
  applyRefinementAsNew,
  archiveItem,
  cancelImport,
  classifyAll,
  getItemContent,
  itemSync,
  listArchived,
  listDuplicates,
  listItems,
  listScanDirs,
  listVerbMap,
  mergeItems,
  pullFromLocation,
  pushToLocation,
  readPlacement,
  refineItem,
  removeScanDir,
  removeSynonym,
  renormalizeVerbs,
  runImport,
  saveMerge,
  type DupGroup,
  type Item,
  type ItemType,
  type MergeResult,
  type RefineResult,
  type ScanDir,
} from "./api";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

const DIRECTIVES = [
  "Generalize: open it beyond a single tool or topic to broader options",
  "Specialize: narrow and sharpen its focus",
  "Tighten guardrails: add validation, error handling, and safety boundaries",
  "Clarify the trigger/description so it activates at the right time",
  "Add concrete examples",
  "Tighten the prose; remove redundancy",
  "Modernize: update to current best practices and APIs",
];
const TOOLS = [
  "Read", "Write", "Edit", "NotebookEdit", "Glob", "Grep", "LSP", "Bash",
  "PowerShell", "Monitor", "WebFetch", "WebSearch", "Agent", "Skill",
];

const searchEl = document.getElementById("search") as HTMLInputElement;
const importBtn = document.getElementById("import") as HTMLButtonElement;
const cancelBtn = document.getElementById("cancel-import") as HTMLButtonElement;
const classifyBtn = document.getElementById("classify") as HTMLButtonElement;
const statusEl = document.getElementById("status")!;
const selbarEl = document.getElementById("selbar")!;
const listEl = document.getElementById("items")!;
const dupesEl = document.getElementById("dupes")!;
const filtersEl = document.getElementById("filters")!;
const sourcesEl = document.getElementById("sources")!;
const verbmapEl = document.getElementById("verbmap")!;
const emptyEl = document.getElementById("empty") as HTMLParagraphElement;
const detailEl = document.getElementById("detail") as HTMLElement;

type TypeFilter = "all" | "skill" | "agent";
type View = "library" | "duplicates" | "archived";

let allItems: Item[] = [];
let archivedItems: Item[] = [];
let scanDirs: ScanDir[] = [];
let dupGroups: DupGroup[] = [];
let verbMap: [string, string][] = [];
let aiOk = false;
let view: View = "library";
let typeFilter: TypeFilter = "all";
let objectFilter: string | null = null;
let query = "";
let selectedId: number | null = null;
const selection = new Set<number>();

const esc = (s: string) =>
  s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
const itemById = (id: number) => allItems.find((i) => i.id === id);

// ---------- sidebar ----------
function renderFilters() {
  const typeCount = (t: TypeFilter) =>
    t === "all" ? allItems.length : allItems.filter((i) => i.item_type === t).length;
  const typeBtn = (t: TypeFilter, label: string) =>
    `<button class="nav${typeFilter === t ? " active" : ""}" data-type="${t}"><span>${label}</span><span class="count">${typeCount(t)}</span></button>`;

  const objects = new Map<string, number>();
  let untriaged = 0;
  for (const it of allItems) {
    if (it.object) objects.set(it.object, (objects.get(it.object) ?? 0) + 1);
    else untriaged++;
  }
  const objBtn = (key: string | null, label: string, n: number) =>
    `<button class="nav sub${objectFilter === key && view === "library" ? " active" : ""}" data-object="${key ?? ""}"><span>${esc(label)}</span><span class="count">${n}</span></button>`;
  let tree: string;
  if (objects.size || untriaged) {
    tree =
      `<div class="nav-head">Objects</div>` +
      objBtn(null, "All objects", allItems.length) +
      [...objects.entries()].sort((a, b) => b[1] - a[1]).map(([o, n]) => objBtn(o, o, n)).join("") +
      (untriaged ? objBtn("__none__", "Untriaged", untriaged) : "");
  } else {
    tree = `<div class="nav-note">Run <b>Classify</b> to group by Object.</div>`;
  }

  const viewBtn = (v: View, label: string, n?: number) =>
    `<button class="nav${view === v ? " active" : ""}" data-view="${v}"><span>${label}</span>${n !== undefined ? `<span class="count">${n}</span>` : ""}</button>`;

  filtersEl.innerHTML =
    `<div class="nav-group">${typeBtn("all", "All")}${typeBtn("skill", "Skills")}${typeBtn("agent", "Agents")}</div>` +
    `<div class="nav-group">${viewBtn("library", "Library")}${viewBtn("duplicates", "Duplicates", dupGroups.length)}${viewBtn("archived", "Archived", archivedItems.length)}</div>` +
    `<div class="nav-group">${tree}</div>`;

  for (const b of filtersEl.querySelectorAll<HTMLButtonElement>("[data-type]"))
    b.addEventListener("click", () => {
      typeFilter = b.dataset.type as TypeFilter;
      renderFilters();
      renderMain();
    });
  for (const b of filtersEl.querySelectorAll<HTMLButtonElement>("[data-view]"))
    b.addEventListener("click", () => {
      view = b.dataset.view as View;
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

function renderSources() {
  const rows = scanDirs
    .map(
      (d) =>
        `<li class="src-item"><span class="badge ${d.item_type}">${d.item_type}</span><span class="src-path" title="${esc(d.path)}">${esc(d.path)}</span><button class="src-rm" data-id="${d.id}" title="Remove">✕</button></li>`,
    )
    .join("");
  sourcesEl.innerHTML =
    `<h3>Custom sources</h3><input id="dir-input" class="dir-input" placeholder="C:\\path\\to\\folder" />` +
    `<div class="add-row"><button id="dir-browse" class="add-btn">📁 Browse…</button></div>` +
    `<div class="add-row"><button id="add-agents" class="add-btn">+ Agents dir</button><button id="add-skills" class="add-btn">+ Skills dir</button></div>` +
    `<ul class="src-list">${rows}</ul>`;
  const input = document.getElementById("dir-input") as HTMLInputElement;
  document.getElementById("dir-browse")!.addEventListener("click", async () => {
    const picked = await open({ directory: true, title: "Pick a skills or agents folder" });
    if (typeof picked === "string") input.value = picked;
  });
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
        syns.map((s) => `<span class="vchip">${esc(s)}<button class="vrm" data-syn="${esc(s)}">✕</button></span>`).join(" ") +
        `</div>`,
    )
    .join("");
  verbmapEl.innerHTML =
    `<details><summary>Verb map (${verbMap.length})</summary><div class="verb-list">${rows}</div>` +
    `<div class="add-row"><input id="vc" class="dir-input" placeholder="Canonical" /><input id="vs" class="dir-input" placeholder="synonym" /></div>` +
    `<div class="add-row"><button id="vadd" class="add-btn">+ Add synonym</button>` +
    `<button id="vrenorm" class="add-btn" title="Re-map existing items through this verb map">Re-normalize items</button></div></details>`;
  document.getElementById("vadd")!.addEventListener("click", async () => {
    const c = (document.getElementById("vc") as HTMLInputElement).value.trim();
    const s = (document.getElementById("vs") as HTMLInputElement).value.trim();
    if (!c || !s) return;
    await addSynonym(c, s);
    verbMap = await listVerbMap();
    renderVerbMap();
  });
  document.getElementById("vrenorm")!.addEventListener("click", async () => {
    try {
      const n = await renormalizeVerbs();
      await load();
      statusEl.textContent = `Re-normalized ${n} item verb(s) through the verb map.`;
    } catch (e) {
      statusEl.textContent = `Error: ${e}`;
    }
  });
  for (const b of verbmapEl.querySelectorAll<HTMLButtonElement>(".vrm"))
    b.addEventListener("click", async () => {
      await removeSynonym(b.dataset.syn!);
      verbMap = await listVerbMap();
      renderVerbMap();
    });
}

// ---------- rows + content ----------
function chips(it: Item): string {
  const c: string[] = [];
  if (it.object) c.push(`<span class="chip obj">${esc(it.object)}${it.sub_object ? " › " + esc(it.sub_object) : ""}</span>`);
  if (it.verb) c.push(`<span class="chip verb">${esc(it.verb)}</span>`);
  if (it.qualifier) c.push(`<span class="chip qual">${esc(it.qualifier)}</span>`);
  if (it.has_variants) c.push(`<span class="chip warn">⚠ variants</span>`);
  return c.join("");
}

function itemRow(it: Item, opts: { select?: boolean; restore?: boolean } = {}): string {
  const cb = opts.select
    ? `<input type="checkbox" class="sel" data-id="${it.id}"${selection.has(it.id) ? " checked" : ""} />`
    : "";
  const restore = opts.restore ? `<button class="restore" data-id="${it.id}">Restore</button>` : "";
  return (
    `<li class="item${it.id === selectedId ? " active" : ""}" data-id="${it.id}">${cb}` +
    `<span class="badge ${it.item_type}">${it.item_type}</span><span class="name">${esc(it.name)}</span>${chips(it)}` +
    `<span class="desc">${esc(it.description)}</span>${restore}</li>`
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

function renderSelbar() {
  const n = selection.size;
  if (n === 0 || view !== "library") {
    selbarEl.hidden = true;
    selbarEl.innerHTML = "";
    return;
  }
  selbarEl.hidden = false;
  const dis = n < 2 ? " disabled" : "";
  selbarEl.innerHTML =
    `<span>${n} selected</span>` +
    `<button id="mc" class="add-btn"${dis}>Merge → Create</button>` +
    `<button id="mr" class="add-btn"${dis}>Merge → Replace</button>` +
    `<button id="clsel" class="add-btn">Classify</button>` +
    `<button id="arch" class="add-btn">Archive</button>` +
    `<button id="clr" class="add-btn">Clear</button>`;
  document.getElementById("mc")!.addEventListener("click", () => startMerge("create"));
  document.getElementById("mr")!.addEventListener("click", () => startMerge("replace"));
  document.getElementById("clsel")!.addEventListener("click", classifySelected);
  document.getElementById("arch")!.addEventListener("click", archiveSelected);
  document.getElementById("clr")!.addEventListener("click", () => {
    selection.clear();
    renderMain();
  });
}

function renderList() {
  const items = visibleItems();
  listEl.innerHTML = items.map((it) => itemRow(it, { select: true })).join("");
  emptyEl.hidden = allItems.length > 0;
  statusEl.textContent = allItems.length ? `${items.length} of ${allItems.length} items` : "";
}

function renderDuplicates() {
  if (!dupGroups.length) {
    dupesEl.innerHTML = `<p class="empty">No duplicates yet — run <b>Classify</b> first.</p>`;
    return;
  }
  const rank = { exact: 0, near: 1 } as const;
  dupesEl.innerHTML = [...dupGroups]
    .sort((a, b) => rank[a.kind] - rank[b.kind])
    .map((g) => {
      const members = g.item_ids
        .map(itemById)
        .filter((x): x is Item => !!x)
        .map((it) => itemRow(it, { select: true }))
        .join("");
      return (
        `<div class="dup-group"><div class="dup-head"><span class="chip ${g.kind === "exact" ? "warn" : "verb"}">${g.kind}</span> <b>${esc(g.key)}</b> <span class="count">${g.item_ids.length}</span></div>` +
        `<ul class="items">${members}</ul></div>`
      );
    })
    .join("");
  statusEl.textContent = `${dupGroups.length} duplicate/similar groups`;
}

function renderArchived() {
  dupesEl.innerHTML = archivedItems.length
    ? `<ul class="items">${archivedItems.map((it) => itemRow(it, { restore: true })).join("")}</ul>`
    : `<p class="empty">No archived items.</p>`;
  statusEl.textContent = `${archivedItems.length} archived`;
}

function renderMain() {
  if (view === "library") {
    dupesEl.hidden = true;
    listEl.hidden = false;
    renderList();
  } else {
    listEl.hidden = true;
    dupesEl.hidden = false;
    if (view === "duplicates") renderDuplicates();
    else renderArchived();
  }
  renderSelbar();
}

// ---------- detail / preview ----------
function closeDetail() {
  selectedId = null;
  detailEl.hidden = true;
  detailEl.innerHTML = "";
  renderMain();
}

async function openDetail(id: number) {
  const it = itemById(id) ?? archivedItems.find((i) => i.id === id);
  if (!it) return;
  selectedId = id;
  renderMain();
  detailEl.hidden = false;
  detailEl.innerHTML =
    `<div class="detail-head"><div class="detail-title"><span class="badge ${it.item_type}">${it.item_type}</span><b>${esc(it.name)}</b></div>` +
    `<button id="detail-refine" class="rf-btn" title="Refactor & improve">✦</button>` +
    `<button id="detail-archive" class="rf-btn" title="Archive">🗄</button>` +
    `<button id="detail-close" class="src-rm" title="Close">✕</button></div>` +
    `<div class="detail-chips">${chips(it)}</div>` +
    (it.description ? `<p class="detail-desc">${esc(it.description)}</p>` : "") +
    `<div class="detail-path" title="${esc(it.library_path)}">${esc(it.library_path)}</div>` +
    `<div class="sync-panel" id="sync-panel"></div>` +
    `<pre class="detail-body">Loading…</pre>`;
  document.getElementById("detail-close")!.addEventListener("click", closeDetail);
  document.getElementById("detail-refine")!.addEventListener("click", () => openRefine(id));
  document.getElementById("detail-archive")!.addEventListener("click", async () => {
    await archiveItem(id, true);
    closeDetail();
    await load();
    statusEl.textContent = "Archived.";
  });
  renderSyncPanel(id);
  const body = detailEl.querySelector(".detail-body")!;
  try {
    body.textContent = await getItemContent(id);
  } catch (e) {
    body.textContent = `Error: ${e}`;
  }
}

// ---------- sync & deploy ----------
async function renderSyncPanel(id: number) {
  const el = document.getElementById("sync-panel");
  if (!el) return;
  try {
    const places = await itemSync(id);
    if (!places.length) {
      el.innerHTML = `<div class="rf-head">Locations &amp; sync</div><div class="nav-note">No tracked locations.</div>`;
      return;
    }
    el.innerHTML =
      `<div class="rf-head">Locations &amp; sync</div>` +
      places
        .map(
          (p) =>
            `<div class="sync-row"><span class="sdot ${p.status}"></span>` +
            `<span class="sync-label" title="${esc(p.abs_path)}">${esc(p.location_label)}</span>` +
            `<span class="sync-status">${p.status.replace("_", " ")}</span>` +
            `<button class="sbtn" data-act="diff" data-pid="${p.id}">Diff</button>` +
            `<button class="sbtn" data-act="push" data-pid="${p.id}">Push →</button>` +
            `<button class="sbtn" data-act="pull" data-pid="${p.id}">← Pull</button></div>`,
        )
        .join("");
    for (const b of el.querySelectorAll<HTMLButtonElement>(".sbtn"))
      b.addEventListener("click", () => onSyncAction(id, Number(b.dataset.pid), b.dataset.act!));
  } catch (e) {
    el.innerHTML = `<div class="nav-note">Sync error: ${esc(String(e))}</div>`;
  }
}

async function onSyncAction(id: number, pid: number, act: string) {
  try {
    if (act === "diff") {
      const [lib, loc] = await Promise.all([getItemContent(id), readPlacement(pid)]);
      detailEl.innerHTML =
        `<div class="detail-head"><div class="detail-title"><b>Library vs location</b></div><button id="sd-x" class="src-rm" title="Back">✕</button></div>` +
        `<div class="rf-head">Library (canonical)</div><pre class="detail-body">${esc(lib)}</pre>` +
        `<div class="rf-head">Location</div><pre class="detail-body dim">${esc(loc)}</pre>`;
      document.getElementById("sd-x")!.addEventListener("click", () => openDetail(id));
      return;
    }
    if (act === "push") {
      await pushToLocation(pid);
      statusEl.textContent = "Pushed library → location (original backed up).";
    } else if (act === "pull") {
      await pullFromLocation(pid);
      await load();
      statusEl.textContent = "Pulled location → library (original backed up).";
    }
    await renderSyncPanel(id);
  } catch (e) {
    statusEl.textContent = `Error: ${e}`;
  }
}

// ---------- refactor & improve ----------
function openRefine(id: number) {
  const it = itemById(id);
  if (!it) return;
  detailEl.hidden = false;
  detailEl.innerHTML =
    `<div class="detail-head"><div class="detail-title"><b>Refactor: ${esc(it.name)}</b></div><button id="rf-x" class="src-rm" title="Cancel">✕</button></div>` +
    `<div class="rf-head">Directives</div>` +
    DIRECTIVES.map((d, i) => `<label class="rf-chk"><input type="checkbox" data-dir="${i}" /> ${esc(d.split(":")[0])}</label>`).join("") +
    `<div class="rf-head">Tools — click to + add / − remove</div>` +
    `<div class="rf-tools">${TOOLS.map((t) => `<button class="rf-tool" data-tool="${t}" data-state="0">${t}</button>`).join("")}</div>` +
    `<div class="add-row"><button id="rf-run" class="primary">✦ Run refine</button></div><p id="rf-status" class="status"></p>`;
  document.getElementById("rf-x")!.addEventListener("click", () => openDetail(id));
  for (const b of detailEl.querySelectorAll<HTMLButtonElement>(".rf-tool"))
    b.addEventListener("click", () => {
      const s = (Number(b.dataset.state) + 1) % 3;
      b.dataset.state = String(s);
      b.className = "rf-tool" + (s === 1 ? " add" : s === 2 ? " remove" : "");
    });
  document.getElementById("rf-run")!.addEventListener("click", () => runRefine(id, it.name));
}

async function runRefine(id: number, name: string) {
  const rfStatus = document.getElementById("rf-status")!;
  if (!aiOk) {
    rfStatus.textContent = "Set a valid OPENAI_API_KEY (then restart) to refine.";
    return;
  }
  const dirs: string[] = [];
  for (const c of detailEl.querySelectorAll<HTMLInputElement>("input[data-dir]"))
    if (c.checked) dirs.push(DIRECTIVES[Number(c.dataset.dir)]);
  const toolsAdd: string[] = [];
  const toolsRemove: string[] = [];
  for (const b of detailEl.querySelectorAll<HTMLButtonElement>(".rf-tool")) {
    if (b.dataset.state === "1") toolsAdd.push(b.dataset.tool!);
    else if (b.dataset.state === "2") toolsRemove.push(b.dataset.tool!);
  }
  if (!dirs.length && !toolsAdd.length && !toolsRemove.length) {
    rfStatus.textContent = "Pick at least one directive or tool change.";
    return;
  }
  rfStatus.textContent = "Refining…";
  try {
    showRefineDiff(id, name, await refineItem(id, dirs, toolsAdd, toolsRemove));
  } catch (e) {
    rfStatus.textContent = `Error: ${e}`;
  }
}

function showRefineDiff(id: number, name: string, res: RefineResult) {
  detailEl.innerHTML =
    `<div class="detail-head"><div class="detail-title"><b>Refined: ${esc(name)}</b></div><button id="rf-x2" class="src-rm" title="Discard">✕</button></div>` +
    `<div class="add-row"><input id="rf-name" class="dir-input" value="${esc(name)} (refined)" /></div>` +
    `<div class="add-row"><button id="rf-save" class="primary">Save (overwrite)</button>` +
    `<button id="rf-savenew" class="add-btn">Save as new</button>` +
    `<button id="rf-back" class="add-btn">Back</button></div><p id="rf-status" class="status"></p>` +
    `<div class="rf-head">Proposed</div><pre class="detail-body">${esc(res.proposed)}</pre>` +
    `<div class="rf-head">Original</div><pre class="detail-body dim">${esc(res.original)}</pre>`;
  const rfErr = (e: unknown) => (document.getElementById("rf-status")!.textContent = `Error: ${e}`);
  document.getElementById("rf-x2")!.addEventListener("click", () => openDetail(id));
  document.getElementById("rf-back")!.addEventListener("click", () => openRefine(id));
  document.getElementById("rf-save")!.addEventListener("click", async () => {
    try {
      await applyRefinement(id, res.proposed);
      await load();
      openDetail(id);
      statusEl.textContent = "Refinement saved (original backed up).";
    } catch (e) {
      rfErr(e);
    }
  });
  document.getElementById("rf-savenew")!.addEventListener("click", async () => {
    const nm = (document.getElementById("rf-name") as HTMLInputElement).value.trim() || `${name} (refined)`;
    try {
      const newId = await applyRefinementAsNew(id, res.proposed, nm);
      await load();
      openDetail(newId);
      statusEl.textContent = "Saved as a new item (original kept).";
    } catch (e) {
      rfErr(e);
    }
  });
}

// ---------- merge & archive ----------
async function startMerge(mode: "create" | "replace") {
  if (selection.size < 2) return;
  if (!aiOk) {
    statusEl.textContent = "Set a valid OPENAI_API_KEY (then restart) to merge.";
    return;
  }
  const ids = [...selection];
  statusEl.textContent = "Merging…";
  try {
    showMergeReview(ids, mode, await mergeItems(ids));
  } catch (e) {
    statusEl.textContent = `Error: ${e}`;
  }
}

function showMergeReview(ids: number[], mode: "create" | "replace", res: MergeResult) {
  detailEl.hidden = false;
  detailEl.innerHTML =
    `<div class="detail-head"><div class="detail-title"><b>Merge → ${mode}</b></div><button id="mg-x" class="src-rm" title="Discard">✕</button></div>` +
    `<div class="detail-path">Sources: ${res.sources.map((s) => esc(s.name)).join(", ")}</div>` +
    `<div class="add-row"><input id="mg-name" class="dir-input" value="${esc(res.sources[0]?.name ?? "merged")} (merged)" /></div>` +
    `<div class="add-row"><button id="mg-save" class="primary">Save ${mode === "replace" ? "(archive sources)" : "as new"}</button></div><p id="mg-status" class="status"></p>` +
    `<div class="rf-head">Proposed</div><pre class="detail-body">${esc(res.proposed)}</pre>`;
  document.getElementById("mg-x")!.addEventListener("click", closeDetail);
  document.getElementById("mg-save")!.addEventListener("click", async () => {
    const name = (document.getElementById("mg-name") as HTMLInputElement).value.trim() || "merged";
    try {
      const newId = await saveMerge(ids, res.proposed, name, mode);
      selection.clear();
      await load();
      openDetail(newId);
      statusEl.textContent = mode === "replace" ? "Merged; sources archived." : "Merged into a new item.";
    } catch (e) {
      document.getElementById("mg-status")!.textContent = `Error: ${e}`;
    }
  });
}

async function archiveSelected() {
  const ids = [...selection];
  for (const id of ids) await archiveItem(id, true);
  selection.clear();
  await load();
  statusEl.textContent = `Archived ${ids.length} item(s).`;
}

async function classifySelected() {
  if (!aiOk) {
    statusEl.textContent = "Set a valid OPENAI_API_KEY (then restart) to classify.";
    return;
  }
  const ids = [...selection];
  if (!ids.length) return;
  statusEl.textContent = `Classifying ${ids.length} selected…`;
  try {
    const s = await classifyAll(ids);
    selection.clear();
    await load();
    statusEl.textContent = `Classified ${s.classified} selected`;
  } catch (e) {
    statusEl.textContent = `Error: ${e}`;
  }
}

// ---------- load + events ----------
async function load() {
  const [items, arch, dirs, ok, vmap, dups] = await Promise.all([
    listItems(),
    listArchived(),
    listScanDirs(),
    aiAvailable(),
    listVerbMap(),
    listDuplicates(),
  ]);
  allItems = items;
  archivedItems = arch;
  scanDirs = dirs;
  aiOk = ok;
  verbMap = vmap;
  dupGroups = dups;
  for (const id of [...selection]) if (!allItems.some((i) => i.id === id)) selection.delete(id);
  classifyBtn.disabled = !aiOk;
  classifyBtn.title = aiOk ? "Classify with AI" : "Set OPENAI_API_KEY to enable";
  renderFilters();
  renderSources();
  renderVerbMap();
  renderMain();
}

function onRowClick(e: Event) {
  const t = e.target as HTMLElement;
  if (t.classList.contains("sel")) {
    const id = Number((t as HTMLInputElement).dataset.id);
    if (selection.has(id)) selection.delete(id);
    else selection.add(id);
    renderSelbar();
    return;
  }
  if (t.classList.contains("restore")) {
    archiveItem(Number(t.dataset.id), false).then(load);
    return;
  }
  const li = t.closest("li.item") as HTMLElement | null;
  if (li?.dataset.id) openDetail(Number(li.dataset.id));
}
listEl.addEventListener("click", onRowClick);
dupesEl.addEventListener("click", onRowClick);

searchEl.addEventListener("input", () => {
  query = searchEl.value;
  if (view !== "library") view = "library";
  renderFilters();
  renderMain();
});

importBtn.addEventListener("click", async () => {
  importBtn.disabled = true;
  cancelBtn.hidden = false;
  cancelBtn.disabled = false;
  // Gate the rest of the UI: the import holds the single DB connection for its
  // whole run, so any other DB-touching command would block the main thread.
  // `body.importing` disables everything except the Cancel button (see styles.css),
  // keeping the main thread free so Cancel is always honored.
  document.body.classList.add("importing");
  statusEl.textContent = "Importing… (scanning locations + tarball)";
  try {
    const s = await runImport(); // resolves on done OR cancel; s.cancelled says which
    await load();
    statusEl.textContent = s.cancelled
      ? `Cancelled — kept ${s.items_new} new (partial, re-runnable) · ${allItems.length} total`
      : `Imported ${s.items_new} new · ${s.variants_flagged} variants · ${allItems.length} total`;
  } catch (e) {
    statusEl.textContent = `Error: ${e}`;
  } finally {
    importBtn.disabled = false;
    cancelBtn.hidden = true;
    document.body.classList.remove("importing");
  }
});
cancelBtn.addEventListener("click", async () => {
  cancelBtn.disabled = true;
  statusEl.textContent = "Cancelling…";
  try {
    await cancelImport();
  } catch (e) {
    statusEl.textContent = `Error: ${e}`;
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

listen<{ done: number; total: number }>("classify-progress", (e) => {
  statusEl.textContent = `Classifying… ${e.payload.done}/${e.payload.total}`;
});
listen<string>("import-progress", (e) => {
  statusEl.textContent = e.payload;
});

window.addEventListener("error", (ev) => {
  statusEl.textContent = `JS error: ${ev.message}`;
});
window.addEventListener("unhandledrejection", (ev) => {
  statusEl.textContent = `Promise error: ${ev.reason}`;
});
load().catch((e) => {
  statusEl.textContent = `Load error: ${e}`;
  listEl.innerHTML = `<li class="item">⚠ Load failed: ${esc(String(e))}</li>`;
});
