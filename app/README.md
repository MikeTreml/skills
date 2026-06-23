# Skill & Agent Library

A desktop app to consolidate the Claude skills and agents scattered across your
machine into one searchable library — then classify, de-duplicate, refactor,
merge, delete, and sync them back out.

Built with **Tauri 2** (Rust core + vanilla-TypeScript frontend) and **SQLite**
(`rusqlite`, bundled). AI features use the OpenAI API (`gpt-4o-mini`).

## What it does

- **Scan & import** — discovers skills/agents in `~/.claude/{skills,agents}`,
  `~/.claude/plugins/marketplaces`, `~/.codex/skills`, project `.claude/{skills,agents}`
  under `~/Repo`, plus any custom folders you add. Copies each item into the
  library and content-hashes it for drift detection. Import is async + cancellable.
- **Classify & dedupe** — an AI classifier tags each item with a canonical
  `Object › Sub — Verb · Qualifier` (13-verb taxonomy + an editable synonym map),
  and surfaces exact/near duplicates.
- **Refactor & improve** — directive checkboxes + tool add/subtract, diff review,
  save-over (with backup) or save as a new variant.
- **Merge** — multi-select (in the Library or Duplicates view), AI-merge into a
  new item, optionally deleting the sources.
- **Delete / restore** — soft-delete moves library copies to `_deleted_backups/`
  and tombstones the record so a re-scan won't bring it back; a Deleted view
  restores them. Your source files at their real locations are never touched.
- **Sync & deploy** — per-item placement panel: status (in_sync/drifted/missing),
  diff, push/pull with backups.

## Run

Prerequisites: [Rust](https://rustup.rs), Node.js, and the
[Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
npm install
npm run tauri dev        # develop
npm run tauri build      # produce an installable build (.msi/.exe on Windows)
```

Set `OPENAI_API_KEY` in the environment to enable the AI features (classify,
refactor, merge). The app reads it at startup.

> Close the app with the window ✕ or Ctrl+C — never force-kill it, as that can
> interrupt a SQLite checkpoint and corrupt the catalog. The catalog is a
> rebuildable index: if it is ever corrupted, delete it and re-import.

## Develop

```sh
npm run build                                   # type-check + bundle the frontend
cargo test --manifest-path src-tauri/Cargo.toml # Rust tests
```

## Layout

- `src/` — frontend (`main.ts`, `api.ts`, `styles.css`)
- `src-tauri/` — Rust core: `commands.rs` (Tauri commands), `db.rs` (SQLite),
  `importer.rs`, `scanner.rs`, `ai.rs`, `dedup.rs`, `taxonomy.rs`, `model.rs`
