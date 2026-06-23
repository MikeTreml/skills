# Milestone 2 — Classify + Duplicates — Implementation Plan

> REQUIRED SUB-SKILL: executing-plans / inline TDD. Steps use `- [ ]`. One tightly-coupled crate → build sequentially.

**Goal:** Tag every item with a canonical `Object / Sub / Verb / Qualifier`, regroup the tree by Object, and surface a duplicates/similar view.

**Architecture:** Deterministic core first (verb-synonym map → canonical verb; dedup grouping) — pure, fully testable with no API. Then the AI classifier (OpenAI API via reqwest, `gpt-4o-mini`) that assigns Object/Sub/Verb/Qualifier from name+description, normalized through the verb map. Then UI (Classify button, Object-grouped tree, Duplicates view).

**Tech:** existing Rust core + `reqwest` (json, rustls) for the API; TS frontend.

---

## Task 1 — Verb-synonym taxonomy (pure, TDD)
**Files:** create `app/src-tauri/src/taxonomy.rs`; `mod taxonomy;` in lib.rs.

- [ ] Seed the 13 canonical verbs + synonym map as a `&[(&str, &[&str])]` const.
- [ ] `canonical_verb(word: &str) -> Option<&'static str>` — lowercase, look up word as canonical or synonym → canonical; None if unknown.
- [ ] Tests: `create` ← new/insert/build/generate/scaffold all map to `Create`; `debug`→`Fix`; unknown (`frobnicate`)→None; case-insensitive.

## Task 2 — DB: classification columns + verb_map table (TDD)
**Files:** modify `db.rs`.

- [ ] Migrate `items`: add `object TEXT, sub_object TEXT, verb TEXT, qualifier TEXT, archived INTEGER NOT NULL DEFAULT 0` (use `ALTER TABLE ... ADD COLUMN` guarded by a pragma check so existing DBs upgrade).
- [ ] `set_classification(conn, id, object, sub, verb, qualifier)`; include the 4 fields + `archived` in `list_items` SELECT and the `Item` struct (model.rs) + api.ts.
- [ ] `verb_map` table (canonical TEXT, synonym TEXT UNIQUE) seeded from taxonomy on init; `list_verb_map`, `upsert_synonym`, `remove_synonym`.
- [ ] Tests: classification round-trips; `list_items` excludes `archived=1` by default; verb_map seed present.

## Task 3 — Duplicate / similar detection (pure, TDD)
**Files:** create `app/src-tauri/src/dedup.rs`.

- [ ] `group_duplicates(items: &[Item]) -> Vec<DupGroup>` where exact = same `(object, sub, verb)` with >1 member; near = same `(object)` different verb. `DupGroup { key, kind: Exact|Near, item_ids }`.
- [ ] Tests: two items `Ax/Form/Create` → one Exact group; `Ax/Form/Create` + `Ax/Form/Review` → Near; different objects → no group.

## Task 4 — AI classifier (OpenAI API)
**Files:** create `app/src-tauri/src/ai.rs`; add `reqwest = { version = "0.12", features = ["json","rustls-tls"], default-features = false }` to Cargo.toml.

- [ ] `AiConfig { api_key }` from `OPENAI_API_KEY` env (fallback: stored setting). Command `ai_available() -> bool`.
- [ ] `classify_item(cfg, name, desc, verbs: &[&str]) -> Classification` — one `gpt-4o-mini` call, JSON-mode prompt: "given name+description, return {object, sub_object, verb (one of <13>), qualifier}". Parse JSON; run `verb` through `canonical_verb` as a guard.
- [ ] Command `classify_all(state)` — iterate unclassified items, call `classify_item`, `set_classification`; batch with a concurrency cap; return progress counts. (No live test in suite; gated behind a key.)
- [ ] Unit-test the prompt builder + JSON parser with a canned response (no network).

## Task 5 — UI: Object tree, Classify button, Duplicates view
**Files:** `api.ts`, `main.ts`, `index.html`, `styles.css`.

- [ ] Sidebar: when items are classified, render an **Object › Sub** tree (counts) as filters, above the type filter; "Untriaged" bucket for unclassified.
- [ ] Top bar: **Classify** button (calls `classify_all`, shows progress, disabled without a key) next to Import.
- [ ] **Duplicates** view toggle: list Exact then Near groups; clicking a member opens the preview; (merge wiring lands in M4).
- [ ] Show `Object · Verb · Qualifier` chips on each row when present.

## Verification
- [ ] `cargo test` green (taxonomy, db, dedup, prompt/parse).
- [ ] `npm run build` clean.
- [ ] Launch; with a key set, Classify tags items; tree groups by Object; Duplicates view lists clusters.

## Notes / scope
- Classification is a **parallel tag** — original names untouched (reversible).
- Verb map is editable + re-runnable (changing it + re-classify re-tags).
- Archive column is added here (used by M4); list views hide archived.
