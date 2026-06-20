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

export interface ScanDir {
  id: number;
  path: string;
  item_type: ItemType;
  enabled: boolean;
}

export const listItems = () => invoke<Item[]>("list_items");
export const runImport = () => invoke<ImportSummary>("run_import");
export const getItemContent = (id: number) => invoke<string>("get_item_content", { id });

export const listScanDirs = () => invoke<ScanDir[]>("list_scan_dirs");
export const addScanDir = (path: string, item_type: ItemType) =>
  invoke<void>("add_scan_dir", { path, item_type });
export const removeScanDir = (id: number) => invoke<void>("remove_scan_dir", { id });
