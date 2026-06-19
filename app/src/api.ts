import { invoke } from "@tauri-apps/api/core";

export type ItemType = "skill" | "agent";

export interface Item {
  id: number;
  item_type: ItemType;
  name: string;
  slug: string;
  description: string;
  category: string | null;
  subcategory: string | null;
  canonical_hash: string;
  library_path: string;
  has_variants: boolean;
}

export interface ImportSummary {
  locations_scanned: number;
  items_found: number;
  items_new: number;
  placements_recorded: number;
  variants_flagged: number;
}

export const listItems = () => invoke<Item[]>("list_items");
export const runImport = () => invoke<ImportSummary>("run_import");
