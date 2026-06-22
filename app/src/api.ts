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
  object: string | null;
  sub_object: string | null;
  verb: string | null;
  qualifier: string | null;
  canonical_hash: string;
  library_path: string;
  has_variants: boolean;
  archived: boolean;
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

export interface DupGroup {
  key: string;
  kind: "exact" | "near";
  item_ids: number[];
}
export interface ClassifySummary {
  classified: number;
  total: number;
}

export const aiAvailable = () => invoke<boolean>("ai_available");
export const classifyAll = (ids?: number[]) =>
  invoke<ClassifySummary>("classify_all", { ids: ids ?? null });
export const listDuplicates = () => invoke<DupGroup[]>("list_duplicates");
export const listVerbMap = () => invoke<[string, string][]>("list_verb_map");
export const addSynonym = (canonical: string, synonym: string) =>
  invoke<void>("add_synonym", { canonical, synonym });
export const removeSynonym = (synonym: string) => invoke<void>("remove_synonym", { synonym });

export const listScanDirs = () => invoke<ScanDir[]>("list_scan_dirs");
export const addScanDir = (path: string, item_type: ItemType) =>
  invoke<void>("add_scan_dir", { path, itemType: item_type }); // Tauri maps camelCase → snake_case
export const removeScanDir = (id: number) => invoke<void>("remove_scan_dir", { id });

export interface RefineResult {
  original: string;
  proposed: string;
}
export const refineItem = (
  id: number,
  directives: string[],
  toolsAdd: string[],
  toolsRemove: string[],
) => invoke<RefineResult>("refine_item", { id, directives, toolsAdd, toolsRemove });
export const applyRefinement = (id: number, content: string) =>
  invoke<void>("apply_refinement", { id, content });

export interface MergeSource {
  id: number;
  name: string;
}
export interface MergeResult {
  proposed: string;
  sources: MergeSource[];
}
export const mergeItems = (ids: number[]) => invoke<MergeResult>("merge_items", { ids });
export const saveMerge = (ids: number[], content: string, name: string, mode: string) =>
  invoke<number>("save_merge", { ids, content, name, mode });
export const archiveItem = (id: number, archived: boolean) =>
  invoke<void>("archive_item", { id, archived });
export const listArchived = () => invoke<Item[]>("list_archived");
