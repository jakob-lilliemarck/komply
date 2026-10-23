---
name: task-authoring
description: |
  Use when writing or reviewing a task brief — the instruction an agent builds
  from.
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
   to medium set of changes a reviewer can take in at once. The test is whether
   they are natural to review together — a whole vertical (a controller through
   its repositories), one new repository, or a handful of one-line nits. Do not
   stack several medium or larger refactors into one brief; they do not read as
   one reviewable change. Keep changes that share a goal together rather than
   cutting them up for size, but do not let a brief become a whole feature. This
   is a balance, not a rule — aim for better, not perfect.

2. **Never leave the application broken.** A finished brief compiles and passes
   every existing test. Unexposed work in progress is fine — a brief may add a
   method nothing calls yet. Exposed behavior must keep working, so a brief that
   changes it updates every caller and test.

3. **Open with the goal and definition of done.** After the skills list, state
   what the brief achieves and why. The goal must be clear enough that any
   reader can repeat it back. Then give a checklist of what "done" means, so a
   reader can stop after these two sections and know whether the rest matters.

4. **Give each public method its exact signature, with no body.** Any method
   another module calls gets its signature in a code block. No bodies, no
   sketches. When the brief adds no public method, say so.

5. **Put migrations in verbatim, under the goal and definition of done.** The
   literal SQL goes in a code block before any prose about it, so the schema
   change is visible first.

6. **Write in plain words.** Write for a reader with little programming
   background: short sentences, no padding. Use a technical term only when it is
   more precise than a plain substitute. Describe behavior as bullets of what
   happens, in order and under what condition — no pseudocode, and no
   line-by-line narration of the code ("first we call X, then we match on Y").
   The full prose rule lives in `@documentation`.

7. **Keep it short.** Stop once the rules above are met. Trust the builder to
   make reasonable calls inside the layer's conventions. Cutting a restated
   convention is fine; cutting a fact the reader must guess is not.

8. **Test every public method.** Each method from rule 4 gets at least one
   bullet naming a case to test — the case, not the assertion. When the brief
   adds no public method, it needs no test section; say so.

9. **Name the skills that apply, and follow them.** List them at the top of the
   brief — any of `@di`, `@config`, `@services`, `@repositories`, `@controllers`,
   `@views`, `@events`, `@jobs`. Their naming, visibility, module layout, error
   handling, and the test rules in the Tests sections of `@repositories` and
   `@services` apply. When the brief cannot match them, revisit the architecture
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
pub(in crate::services::foo) async fn run(args: &Args) -> Result<(), Error>;
```

## Behavior
Plain-language bullets, in order, covering what happens and under what
condition.

## Tests
Omit when there are no public methods. Otherwise one bullet per method or case.

## Notes / out of scope
Omit when empty. Parked items with the reason, and anything else worth flagging.
````

## Service scaffolding

The first brief to add code to a service also creates that service's module
shell — `mod.rs` and `registrations.rs` — and adds only the parts it needs. The
layout lives in `@services` and `@repositories`.

## Creating tasks

Create a brief with the `create-task` tool: `title`, `brief`, `dependencies`,
and `epic`.

- **Group related work under an epic name.** Put the feature name in the `epic`
  field on every task of that work. Never put the epic in the `title` — the card
  renders the epic prefix from the field, so a title that repeats it shows twice.
- **Leave `epic` unset for standalone work** — a one-off fix or a review finding.
- **Express dependencies in the `dependencies` field.** When planning a series of
  tasks, work out which ones must land before others and record them there as
  task IDs — never as prose in the brief. A dependency is code a task needs, not
  file ordering: overlapping edits to one file are a merge conflict, not a
  dependency.
- **The `## Deliver` step is appended for you.** Never write one into a brief.
- **Flag maintainer steps** in the brief and its definition of done — migrations
  and the `.sqlx` cache refresh.

## Self-check

Re-read the brief as an agent who never saw the conversation. Can it name every
file it changes, every public signature it produces, what each broken caller
should now pass, and the commands that prove it done? Does it leave the
application compiling, every existing test passing, and every exposed behavior
still working? "It should be obvious" is not an answer — write it down.

## Related skills

- `@brain-dump` — refining an idea into settled work.
- `@epic-authoring` — writing the epic charter a brief may belong to.
- `@repositories` and `@services` — the tests a brief calls for (their Tests
  sections).
