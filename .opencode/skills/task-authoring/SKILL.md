---
name: task-authoring
description: |
  Use when writing or reviewing a task brief under `.dev/planning/` — the
  instruction a builder acts on.
---

# Writing a task brief

Write a brief that whoever builds it can act on from the brief and the
repository alone.

## Self-containment

The reader never sees the conversation, the pull request, or the intent you did
not write down. The brief works only if that agent can build, verify, and finish
without asking a question.

Write down what the reader cannot find by looking around:

- the exact public signatures to produce;
- the files and existing symbols the change touches — including the call sites
  and tests a changed signature breaks, and what each should now pass;
- any decision that is a choice rather than the consequence of an existing
  convention.

Leave out what the reader can find. The layer skills hold the conventions and
the codebase shows the current shape, so cite a skill instead of copying it in.

The line is *guessable* versus *discoverable*. If a good agent could land on the
wrong answer by guessing, write it down. If the code or a skill answers it
plainly, leave it out.

## Rules

1. **One reviewable change per brief.** Treat a brief as a pull request: a small
   to medium set of changes a reviewer can take in at once. A whole vertical — a
   CLI path through its service and repository — is fine. Do not stack several
   medium refactors into one brief. Keep changes that share a goal together.

2. **Never leave the application broken.** A finished brief compiles and passes
   every existing test. Unexposed work in progress is fine — a brief may add a
   method nothing calls yet. Exposed behaviour must keep working, so a brief that
   changes it updates every caller and test.

3. **Open with the goal and definition of done.** After the skills list, state
   what the brief achieves and why. Then give a checklist of what "done" means,
   so a reader can stop after these two sections and know whether the rest
   matters.

4. **Give each public method its exact signature, with no body.** Any method
   another module calls gets its signature in a code block. No bodies, no
   sketches. When the brief adds no public method, say so.

5. **Put migrations in verbatim, under the goal and definition of done.** Komply
   has no database, so most briefs say `None`. If a brief ever adds one, the
   literal SQL goes in a code block before any prose.

6. **Write in plain words.** Write for a reader with little programming
   background: short sentences, no padding. Use a technical term only when it is
   more precise than a plain substitute. Describe behaviour as bullets of what
   happens, in order and under what condition — no pseudocode, and no
   line-by-line narration of the code.

7. **Keep it short.** Stop once the rules above are met. Trust the builder to
   make reasonable calls inside the layer's conventions. Cutting a restated
   convention is fine; cutting a fact the reader must guess is not.

8. **Test every public method.** Each method from rule 4 gets at least one
   bullet naming a case to test — the case, not the assertion. When the brief
   adds no public method, it needs no test section; say so.

9. **Name the skills that apply, and follow them.** List them at the top of the
   brief: any of `@config`, `@services`, `@repositories`, `@documentation`,
   `@review-branch`. Their naming, visibility, module layout, error handling,
   and test rules apply. When the brief cannot match them, revisit the design
   before writing the brief rather than writing an exception.

## Brief shape

Use these sections, in this order.

````markdown
## Skills
The skills whose rules apply to this brief.

## Goal
One to three sentences in plain words: what this delivers and why.

## Definition of done
Concrete, checkable bullets.

## Migrations
Omit when there are none. Otherwise the verbatim SQL.
```sql
-- verbatim SQL
```

## Public API
"none" when the brief adds no public method. Otherwise the exact signatures.
```rust
// src/path/file.rs — what changed
pub fn run(args: &Args) -> Result<(), Error>;
```

## Behavior
Plain-language bullets, in order, covering what happens and under what
condition.

## Tests
Omit when there are no public methods. Otherwise one bullet per method or case.

## Notes / out of scope
Omit when empty. Parked items with the reason, and anything else worth flagging.
````

## Creating tasks

Tasks are files, not board cards.

- Write each brief to `.dev/planning/<epic>/tasks/T<n>-<slug>.md`.
- Add it to the epic's task table in `.dev/planning/<epic>/README.md`.
- Record dependencies in a `Depends on` line or a dependency table in the epic
  doc, never as prose hidden in the brief. A dependency is code a task needs,
  not file ordering: two tasks editing one file is a merge conflict, not a
  dependency.
- Never write a `## Deliver` section.
- Update `.dev/planning/README.md` when an epic is added or completed.

## Self-check

Re-read the brief as an agent who never saw the conversation. Can it name every
file it changes, every public signature it produces, what each broken caller
should now pass, and the commands that prove it done? Does it leave the
application compiling, every existing test passing, and every exposed behaviour
still working? "It should be obvious" is not an answer — write it down.

## Related skills

- `@brain-dump` — refining an idea into settled work.
- `@services`, `@repositories`, `@config`, `@documentation` — the layer rules a
  brief cites.
