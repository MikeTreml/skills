# Skill & Agent Library — Design Spec

_Date: 2026-06-18 · Status: approved design, ready for implementation planning_

## Problem

Skills (and agents) are scattered across many locations on this machine —
`~/.claude/skills`, `~/.claude/plugins/marketplaces/**`, `~/.claude/agents`, plugin
session dirs, `~/.codex`, and individual project `.claude/skills` folders. There is
no single place to see what exists, where each thing lives, whether copies have
drifted, or to reconcile the many same-named-but-different variants (an existing
inventory found 2,758 `SKILL.md` files → 2,608 unique, with **125 names shipping in
multiple different versions**, e.g. `babysit` × 8).

We want a **desktop library app** that holds canonical copies of skills and agents,
organizes them into a browsable category tree, keeps each real location in sync
(both directions, safely), and uses AI to merge/refactor duplicate or hand-picked
items into clean canonical versions.

## Goals

- One canonical, browsable library of every skill and agent.
- A category → subcategory → item **tree**, AI-seeded and user-editable.
- Per-item **sync** with each real location it lives in — bidirectional, diff-first,
  never silently clobbering or deleting.
- **AI merge** that (a) reconciles same-named variants and (b) fuses hand-picked
  items, then refactors the result into clean `SKILL.md` form.
- Seed on first run from the existing 2,600-skill tarball **and** a live scan of this
  machine.

## Non-goals (v1)

- Cloud sync / multi-machine sync / accounts.
- Editing skill *content* in a rich editor beyond a manual textarea on merge results.
- Publishing to a marketplace or registry.
- Versioned history/undo beyond the safety backups taken on each write.

## Decisions (locked during brainstorming)

| Topic | Decision |
|---|---|
| Form factor | **Desktop app — Tauri** (Rust core + web frontend). |
| Sync model | **Bidirectional with per-location diff.** Library holds the reference copy; sync detects drift either way and applies the user-chosen direction. Never deletes a real folder. |
| Merge scope | **Both** — reconcile same-name variants AND fuse hand-picked items. One engine, then AI refactor to clean form. |
| Categorization | **AI auto-classify on import**, user override. Tree = category → subcategory → item. Low-confidence → `Untriaged`. |
| AI backend | **OpenAI API** (user's `OPENAI_API_KEY`). `gpt-4o-mini` for classify, refactor, and merges. |
| Initial seed | **Import the 2,600-skill `skills-deduped.tar.gz` AND live-scan this machine**, reconciling both. |
| Item granularity | An item is the **whole folder** (`SKILL.md` + `references/`, `scripts/`, …), content-hashed — not just the `SKILL.md`. |
| Agents | Treated identically to skills via a `type` field (`skill | agent`). |

## Architecture

Three layers (see brainstorm mockups in `.superpowers/brainstorm/`):

**Desktop UI (web frontend in Tauri webview)**
- Library Browser — the tree + item list + search + Skills/Agents filter.
- Item Page — canonical copy on the left, per-location sync panel on the right.
- Merge Workbench — pick sources → AI merge+refactor → review diff → save canonical.

**Rust core (exposed as Tauri commands)**
- Scanner — walks each registered location, finds `SKILL.md` / agent `.md`, hashes
  whole folders.
- Library Store — SQLite catalog + copied item folders under one library root.
- Sync Engine — drift detection, per-location diff, safe apply (dry-run, backup,
  atomic replace).
- Classifier (AI) — assigns category/subcategory on import (`gpt-4o-mini`, batched).
- Merge Engine (AI) — fuses sources, then refactors to clean `SKILL.md`.

**Outside the app**
- Real locations (each a sync target), the tarball seed, the OpenAI API.

## Data model (SQLite)

- **`items`** — `id, type (skill|agent), name, slug, description, category,
  subcategory, canonical_hash, library_path, has_variants, created_at, updated_at`
- **`locations`** — `id, label, root_path, kind (claude-skills | marketplace |
  agents | project | codex | tarball), enabled, last_scanned`
- **`placements`** — per-location state powering the sync panel:
  `id, item_id, location_id, rel_path, location_hash,
  status (in_sync | location_newer | library_newer | missing | conflict),
  last_scanned`
- **`variants`** — versions captured before reconcile, feeding the merge workbench:
  `id, name, type, source_path, hash, content, location_id`
- **`taxonomy`** — editable: `id, category, subcategory`

Library files live under a single root, e.g.
`library/<category>/<subcategory>/<name>/…` containing the whole item folder.

### Status derivation

For each `placement`, status is derived by comparing `items.canonical_hash` with a
freshly computed hash of the folder at `location.root_path/placement.rel_path`:

- equal → `in_sync`
- folder missing → `missing`
- differ, and library `updated_at` > location mtime → `library_newer`
- differ, and location mtime > library `updated_at` → `location_newer`
- differ, both changed since last common sync point → `conflict` (diff required)

## Sync engine — safety rules

Every write to a real location MUST:
1. Compute and show a **dry-run diff** first.
2. **Back up** the existing target folder (timestamped) before overwriting.
3. Use **atomic replace** (write to temp, swap).
4. **Never delete** a real folder; "missing" is resolved only by push (create), and
   removing a placement only unlinks it from the library — it never rm's user files.

Directions, chosen per location: **pull** (location → library), **push** (library →
location), **skip**. `conflict` requires an explicit choice after viewing the diff.

## Merge engine

Inputs: a set of sources — either the captured `variants` of one name, or
hand-picked `items` (possibly different names). Steps:

1. Build a prompt containing each source's frontmatter + body and provenance notes.
2. Call Claude (model selectable; default Sonnet) with a **strategy** (default:
   union + dedupe + refactor) to produce one merged `SKILL.md` plus a rationale.
3. Render a **diff** of merged-vs-primary-source; show estimated token cost.
4. User can Re-merge (different model/strategy), Edit manually, Discard, or **Save as
   canonical** — which updates the `items` row, writes the library folder, and marks
   affected placements for optional sync-out.

## Classification

On import, batch items through Haiku: input = name + description (+ truncated body),
output = `{category, subcategory, confidence}` constrained to the current taxonomy
(model may also propose a new subcategory). Confidence below threshold → `Untriaged`.
The taxonomy is user-editable; re-tagging can be re-run anytime.

## Build order (v1 → later, riskiest last)

1. **Scaffold + scan + import** — Tauri shell, SQLite, scanner, tarball seed + live
   scan; populate `items` / `locations` / `placements`.
2. **Tree + browse + item page** — read-only UI: tree, list, item detail.
3. **AI classify** — Haiku batch tagging, editable taxonomy, Untriaged bucket.
4. **Sync engine** — drift detection, per-location diff, push/pull with backups +
   dry-run.
5. **Merge engine** — variant reconcile + hand-picked fuse, AI merge→refactor→diff→
   save canonical.

Each milestone is independently useful.

## Open questions / risks

- **Stale-profile paths:** the seed tarball's catalog references a `C:\Users\Treml`
  profile. Import must treat tarball entries as content sources, not as live
  locations — only this machine's scanned folders become `locations`.
- **Scan scope:** which roots are scanned by default vs. added manually needs a small
  default list (the `kind` enum) plus an "add location" affordance.
- **Conflict detection** needs a stored "last common sync point" hash per placement to
  distinguish `library_newer`/`location_newer` from true `conflict`; v1 may simplify
  to mtime heuristics and always offer a diff.
- **API key handling:** stored via OS keychain (Tauri) rather than plaintext.
