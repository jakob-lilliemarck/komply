---
name: skill-authoring
description: |
  Use when writing or reviewing a skill. Explains what a skill is and how to
  write one that stands alone.
---

# Writing a skill

A skill is instructions for one job. It is read later, away from the
conversation that produced it, so it has to stand alone.

## How, not what or why

A skill is prescriptive of *how*: how to review, how to write a module, how to
run a migration. It makes no judgement about *what* to do or *why* it should be
done — that belongs to a task, an epic, or a plan.

## Structure, patterns, process

A skill carries the architecture and the working method: the structure of the
code, the patterns to follow, the process to run. None of that is tied to a
business domain. "A repository", "a review", "a service" mean the same thing
whether the business is billing, logistics, or something not yet named; the same
skill holds for a new project and an old one.

So a skill never documents the domain, and it makes no assumption that the code
or the docs exist yet. An empty project is enough; the skill says how to build
it. Domain knowledge — this system's tables, entities, and rules — belongs in
`docs/`.

## When a skill is the right home

A skill is for a repeatable procedure: a job an agent does at task time, with
rules it could get wrong by guessing.

| Home | What belongs there |
|---|---|
| `AGENTS.md` | Rules that apply to every change |
| `docs/` | How this particular system works — its domain and decisions |
| `.opencode/skills/<name>/SKILL.md` | How to do one job, in any domain |
| `docs/tasks/` | One unit of work — the what and the why |

Do not create a skill that only restates a doc or `AGENTS.md`; link to the home
instead. If the content is domain knowledge, write a doc. If it is a unit of
work — a what and a why — write a task.

## Give it one job

Give each skill one job, and name it after that job: `repositories`, `distill`,
`events`. Test it as a single sentence — *this skill tells you how to ___*. When
you can finish that sentence in a phrase, the scope is right. When it takes two,
split the skill.

Topics are not jobs. `services` covers naming, DI, errors, events, and jobs, but
its job is still "write a service". Split only when the parts can be done
independently.

A skill about repositories that also tells how services call them is two skills:
keep the repository rules, and move the calling rules to `@services`.

## Make it stand alone

Assume the reader has this skill, the language and tools it names, and any skill
this one links to. They do not have the code, the docs, or the conversation that
produced the skill.

Write down what they could get wrong by guessing: the rule, the exact names, the
file layout, the edge case. State the structure in full; never defer to a tree
that may not exist yet.

Do not lean on the conversation: no "as discussed", no "see the PR".

## Put each rule in one home

Each rule has one home: a skill, `AGENTS.md`, or `docs/`. Where a rule belongs
somewhere else, link to it with `@` and the skill's name (`@services`) or a
markdown link. Repositories own repository rules; services own service rules;
project-wide invariants belong in `AGENTS.md`.

For example, the services skill explains the order in which a service registers
its providers. A provider function is defined in `@di`, so the skill could say:

> The service's `register` calls each sub-repository provider, then its own
> provider. The provider functions are defined in `@di`.

Within a skill, state each rule once, in the section where the reader needs it.
A rule needed in two sections lives in the first one and is referred to from the
other.

A link is a pointer, not an inclusion: the reader must load the linked skill to
get the detail. State enough to act, then link.

## Write instructions

Write short sentences in simple words. The prose should be followable by someone
with little programming background; code samples may assume the language. Choose
the exact word — "repository" and "migration" name their subject, while
"leverage" and "seamless" stay vague. The full prose rule lives in
`@documentation`.

Write each rule as the action the reader takes. The instruction names the move,
and following it produces the result. For example, a rule reads:

> Store entities in one batch.

Prefer the imperative. A declarative rule is fine when it states the invariant
directly. Do not explain why the rule is good unless the reason changes what the
reader does.

## Shape

Use the medium that carries each part best; do not force all three into every
section.

- **Prose** carries the reasoning and the link between ideas.
- **Code** shows a pattern exactly — concrete code beats pseudocode.
- **Bullets** list parallel items: options, rules, names.
- **Tables** compare several things on the same axes.

A section usually opens with a line or two of prose, then the example, list, or
table that makes it concrete.

## Skeleton and location

Every skill lives at `.opencode/skills/<name>/SKILL.md` and has the same frame:
frontmatter, title, one or two sentences of purpose, then sections. The
frontmatter must start and end with `---`; a malformed block makes the skill
disappear from the list.

The `description` is the selection surface: an agent chooses between skills from
the name and description. Say when to use the skill and what it is for, in plain
words — one or two lines.

```markdown
---
name: <lowercase-hyphenated>
description: |
  Use when <the situation that triggers this skill>. <What the skill is for.>
---

# <Title>

One or two sentences: what this skill is for.

## <A part of the thing>

<Short prose, then an example.>

## <The next part>

## Related skills

<Optional: the skills a reader should load for rules this one links to.>
```

`name` is the lookup key and what `@` references resolve to. Keep the directory
name equal to it so the path and the link agree. The loader strips the
frontmatter and injects only the body, so instructions belong in the body.
Verify with the `skill` tool: list the skills, then load this one by name.

## Review it

Use this checklist before calling a skill finished:

- Can you finish "This skill tells you how to ___" in one phrase?
- Does the description make an agent pick this skill for the right task, and
  not for the adjacent one?
- Does the skill prescribe *how*, without deciding *what* or *why*?
- Does it carry structure, patterns, and process — not domain knowledge?
- Could an empty project use it, with no code or docs already written?
- Is every name, path, signature, and edge case a reader could guess wrong
  written down?
- Is any rule also stated in another skill, `AGENTS.md`, or `docs/`? Decide its
  one home and link the rest.
- Is every rule phrased as the move the reader makes?
- Do the examples give the exact names and signatures, not sketches?
- Does every `@` reference point to a real skill?
- Can you delete a sentence without losing a rule?

## Stop

Cover each rule once, with the smallest example that makes it concrete. A skill
is as long as the rules the reader must not guess — no longer, no shorter.
