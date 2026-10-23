---
name: brain-dump
description: |
  Use when refining a user's raw idea — a brain dump, a list of thoughts, or a
  message — into settled work. Read the code, question the idea, judge it, then
  write it to the board.
---

# Handling a brain dump

A user's ideas arrive raw: a `BRAINDUMP.md`, a list of thoughts, or a message
thrown at you in chat. An entry may be a one-line idea or a written-out request.
The job is to turn each entry into work the board can run.

Never edit a source file the user gives you; it is their input. Ideas from chat
have no file to touch.

## Aim for simple code

Weigh every idea by the simplicity of the code it lands as. A big idea is
welcome; try hard to contain it in a simple concept. Some code cannot be simple,
at least not up front — but that is the rare case, not the excuse. A change can
be large and still leave the code simple.

Simple is not "few lines". Simple means:

- loose coupling — the work asks little of the rest of the system;
- clear encapsulation — it keeps its own details to itself;
- logic a reader can follow — the module makes sense without reading the whole
  system.

Simple code is modular. Aim for the design where one module can be reasoned
about on its own.

## The process

Work through the ideas one at a time. For each:

1. **Read the code first.** Find what the idea touches today. Cite the actual
   code; never assert from memory.
2. **Question it.** A thin idea is not planable yet. Ask what the user wants and
   why, and offer two to four concrete options with a recommendation.
3. **Map it.** Name the domain concepts and data structures it touches, say how
   it fits — or fights — the current shape, and how it lands as a simple concept.
4. **Judge it.** Say whether it is a good or bad idea given the business context
   and the state of the codebase. Be critical, not paranoid: if it is sound, say
   so and move on.
5. **Size it.** Say how to carry it out, what it costs, what it breaks, and the
   alternatives.
6. **Pick the planning method** (below), read the matching skill, and write the
   artifact to the board.

## Pick the planning method

An idea lands as one of three shapes. A **single task**, or a **series of tasks**
grouped under one epic name, is written per `@task-authoring`; a **relay epic** —
larger work that steers itself by goal and bounds — is written per
`@epic-authoring`. A series names every task now; a relay epic leaves each leg to
choose its next slice.

## Working with the user

- The user proposes; the agent refines. Do not invent work uninvited.
- Decide one idea at a time: discuss, decide, write, then move on.
- Lock decisions and list open questions; resolve them before writing.
- Park what is out of scope, with the reason, and only when it leaves no mess.

## Blast radius

For each idea to be built, state what it touches and what it costs: the files and
modules; any migration it needs and the query-cache refresh that follows; API or
JSON shape changes a caller would feel; and any service-boundary or cross-service
coupling risk. Apply the project's invariants — append-only storage, service
boundaries, the migration workflow — from `AGENTS.md` rather than from memory.

## Hand off

Open the skill for the chosen method and write the artifact with it:
`@task-authoring` for tasks, `@epic-authoring` for a relay epic. Then create the
work on the board.
