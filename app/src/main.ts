import { listItems, runImport, type Item } from "./api";

const statusEl = document.getElementById("status")!;
const listEl = document.getElementById("items")!;
const importBtn = document.getElementById("import") as HTMLButtonElement;

function render(items: Item[]) {
  listEl.innerHTML = "";
  for (const it of items) {
    const li = document.createElement("li");
    const variant = it.has_variants ? " ⚠ variants" : "";
    li.textContent = `[${it.item_type}] ${it.name}${variant} — ${it.description}`;
    listEl.appendChild(li);
  }
  statusEl.textContent = `${items.length} items in library`;
}

async function refresh() {
  render(await listItems());
}

importBtn.addEventListener("click", async () => {
  importBtn.disabled = true;
  statusEl.textContent = "Importing…";
  try {
    const s = await runImport();
    statusEl.textContent = `Scanned ${s.locations_scanned} locations · ${s.items_new} new · ${s.variants_flagged} variant-flagged`;
    await refresh();
  } catch (e) {
    statusEl.textContent = `Error: ${e}`;
  } finally {
    importBtn.disabled = false;
  }
});

refresh().catch((e) => (statusEl.textContent = `Error: ${e}`));
