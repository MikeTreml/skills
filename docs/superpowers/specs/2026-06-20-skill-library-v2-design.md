# Skill & Agent Library — v2 Design (expanded scope)

_Date: 2026-06-20 · Builds on [2026-06-18 v1 spec](2026-06-18-skill-library-app-design.md). Milestone 1 shipped._

## Why v2

M1 delivered scan + import + a real main form (browse, search, type filter, preview, custom scan dirs). The app is now growing from a cataloguer into a **skill/agent workbench**: classify into a canonical taxonomy, find duplicates/near-duplicates, refactor/improve items with guardrails, and merge. Everything applies to **both skills and agents** (the `type` field already distinguishes them).

## Verified constraints (drive deploy/sync)

From code.claude.com (2026-06-20), see [[claude-skill-agent-naming]]:
- **Skills must use `SKILL.md`**; the descriptive name is the **directory**. Store inventory freely, but deploy = `<descriptive>/SKILL.md`.
- **Agent filenames are cosmetic**; identity is frontmatter `name:`. Deploy = copy the `.md` into `.claude/agents/`.
- `allowed-tools`/`tools` syntax: comma/space/YAML list; scoped `Bash(git add *)`, `Read(src/**)`, MCP `mcp__server__tool`.

## Canonical naming taxonomy (the dedup lever)

Noun-first so like sorts with like; PowerShell-approved-verbs discipline so synonyms collapse:

```
Object [ › Sub-object ]  —  Verb  [ · Qualifier ]
```
e.g. `Ax › Form — Expert`, `Ax › Enum — Create`, `Twilio — Configure`, `Code — Review`.

**13 canonical verbs** (synonyms normalize to these): Create (new/insert/add/build/generate/scaffold/make/author/draft/develop/implement), Analyze, Review (audit), Explain (document/summarize/guide), Refactor, Convert (migrate/translate/format/import/export), Optimize, Test (validate/verify/lint), Fix (debug/repair/resolve), Search (find/extract/query/classify/detect), Configure (set up/install/integrate), Manage (deploy/monitor/run/sync), Design (plan/architect/compare/recommend). Unmapped verbs are kept and flagged "uncanonical" (extensible, not limiting). Qualifiers: roles (Expert, Specialist, Reviewer, Assistant, Guide) + scope (CRUD, Deep, Quick). The synonym→canonical map is stored in an **editable table** and re-runnable.

**Dedup from the canonical form:** same `(Object, Sub, Verb)` = duplicate; same `Object›Sub`, related Verb = near-duplicate/overlap; sort by Object to eyeball a whole surface.

## Feature set & scope (confidence-checked)

**Core (this v2):**
1. **Classification** — AI (Haiku) assigns `Object / Sub / Verb / Qualifier` per item from name+description, normalized via the verb map; stored as a parallel tag (original names untouched). Tree regroups by Object.
2. **Duplicates & similar view** — clusters by `(Object, Verb)`; exact vs near.
3. **Refactor & Improve** (per item, skills+agents) — AI refine with directive checkboxes: Generalize / Specialize, **Tools add/subtract** (the toggle list above), Tighten guardrails, Clarify trigger, Add examples, Tighten prose, Modernize. Produces a diff → review → save / save-as-variant.
4. **Multi-select merge** — modes: **Create** (new combined, keep originals) and **Replace** (combined canonical, originals **archived**). Needs an **Archive** state (soft-delete, hidden, restorable).
5. **Non-blocking import + progress**; **native folder picker** for sources.

**Deferred (own milestones):** per-location bidirectional **sync** (v1 M4, the deploy transform above) ; validate/lint; export/deploy; search-by-tool; tags. **Out (scope creep):** full in-app rich editor, git versioning of the library.

## AI backend

Anthropic API (Haiku classify, Sonnet/Opus refactor+merge) via the user's `ANTHROPIC_API_KEY`. New Rust component: an HTTP client (`reqwest`) + key from env/OS keychain + batched calls + JSON parsing. This is the largest new dependency in v2.

## Milestone roadmap (v2)

- **M2 — Classify + Duplicates** (this plan): verb-synonym table, canonical normalization, Object/Verb/Qualifier columns, AI classifier (Anthropic API), classify action, Object-grouped tree, duplicates/similar view. _Deterministic core (verb map, normalization, dedup) is testable without the API; the API call is the one live piece._
- **M3 — Refactor & Improve**: directive-driven AI refine + tool add/subtract, diff/save/save-as-variant.
- **M4 — Merge & Archive**: multi-select merge (create/replace), archive state + restore.
- **M5 — Sync & Deploy**: bidirectional per-location sync with the SKILL.md/agent deploy transform; non-blocking import; folder picker (folder picker may land earlier as polish).

Each milestone gets its own plan and produces working, tested software.
