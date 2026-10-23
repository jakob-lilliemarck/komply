---
name: documentation
description: |
  Use when writing or reviewing documentation: markdown docs, docstrings, and
  code comments. Covers plain-language writing, the three documentation levels
  (docs, docstrings, comments), contract-first docstrings, and keeping
  documentation local to the code it describes. Do NOT use for task files
  under `.dev/planning/` — see the task-authoring skill.
---

# Documentation

## The one rule: plain language

Write for someone with little programming background: plain words, short
sentences, no padding.

Specialized terms earn their place only by being more precise than a plain
substitute — sometimes a topic needs one (dependency injection, database
indexes, semantic search are examples, not the only ones). Jargon, though,
is often "power language": a way to keep people out rather than to say
something exactly. Precision is good; sweeping jargon is not.

## Three levels, one rule

| Level | What it's for |
|---|---|
| Markdown docs | The big picture: how a system or a large part of it works |
| Docstrings | The contract of a function, struct, or constant |
| Code comments | The "why" behind a specific implementation choice |

Choose the level by what you need to say:

- The system, or how separate parts interact → markdown doc.
- How to use a function, struct, or constant → docstring.
- Why this code is written this way → code comment.

## Markdown docs

Markdown docs give the overarching idea: what the system does, the order and
meaning of its operations, how larger pieces fit together. This is the only
place interaction between separate parts is documented — it has no home in a
docstring or comment.

- Collect them under `docs/`.
- Keep `docs/index.md` up to date. It is the entry point to the docs: every
  document under `docs/` is linked from it.
- A `README.md` introduces the directory it sits in — the repository root, or a
  subsystem such as a set of benches. It is an introduction, not a
  documentation document: for anything the docs cover, it refers to
  `docs/index.md` instead of repeating it.
- One topic per document, and no overlap between documents. Each document
  carries its own weight; a thing is explained in exactly one place.
- When a second document needs the same content, extract that part into its
  own document and refer to it from both. Needing to repeat something is the
  signal to break it out, not to copy it.
- Refer to another document with a markdown link so it is clickable:
  `[document name](path/to/document.md)`.
- When the facts a document owns change, update that document — and only that
  document.
- When a whole document is no longer relevant, delete it and remove every
  reference to it: `docs/index.md`, other documents, and any skill that links
  it.
- Write plainly, with little narration.
- Break up the text with code blocks: directory trees, flow-charts or
  sequences, and the signatures of key public functions only.

## Docstrings

A docstring is `///` (or `//!` for a module); a code comment is `//`.

A docstring is the contract of the code it sits above. It tells a caller
everything they need to know to use it:

- how to construct the struct or call the function,
- what to pass, and what each argument means,
- what comes back — errors are part of the return value in Rust (`Result`),
- any side effect,
- any way the code can panic (`unwrap`, `expect`, `assert`).

It never describes how the code achieves that.

```rust
/// Returns the latest saved filter for `user_id`, or `None` when the user
/// has saved no filter yet. Never modifies the saved filter.
pub async fn latest_for(&self, user_id: Uuid) -> Result<Option<Filter>, Error>;
```

Keep them brief. Doc examples are verified by the full `cargo test` suite,
which includes doc tests.

## Code comments

A code comment is `//` and explains implementation details and choices,
placed as close to the relevant lines as possible. It answers "why is it like
this?" — the non-obvious decision, the constraint, the gotcha — not "what
does this code do", which the code already says.

```rust
// Only non-empty facet vectors restrict the search: an empty `Vec` would
// otherwise resolve to an empty candidate set and return zero results.
```

Keep them short. A comment that rambles is as unhelpful as none.

## Docstrings and comments stay in their own block

A docstring or comment describes only the function, struct, or constant it
lives in. It never mentions call sites, never references surrounding code,
never documents another block.

Why: a note in file C about a concept in file B will drift out of sync, and
nobody will notice until it has cost them a wrong turn. Keeping documentation
local keeps it honest. Interaction between separate parts belongs in a
markdown doc.

## Document what is, not what changed

Documentation describes the code and the system as they are now. It never
narrates a change: no "changed from ...", no "used to ...", no "new" or
"renamed", no diff-like notes. A reader who only sees `main` has no view of
the change, so such a note is noise at best and misleading at worst. If it
matters, it belongs in the commit message or pull request.

When a change alters the code, update the documentation to state the new
truth. Do not annotate the old one.

## When reviewing

Check the documentation against the rules above. Flag in particular:

- the same explanation living in two documents,
- a document that is missing from `docs/index.md`,
- a docstring or comment that names a call site or another block,
- a note that describes a change instead of the code,
- a comment that rambles or restates the code.
