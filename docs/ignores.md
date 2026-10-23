# Ignores

Two filters decide which files are sent. They use different bases, so they do
not clash.

## `.complyignore`

`.complyignore` sits at the working-directory root, next to `.architecture/`.
Its patterns are relative to that root.

```text
# generated files
target/
node_modules/
*.lock
```

- One glob per line.
- Blank lines are ignored.
- A line starting with `#` is a comment.
- One trailing `/` is removed; the pattern still applies to a directory.
- There is no negation and no `!` support.

A directory that matches an ignore prunes its whole subtree.

Komply always adds `.git` and `.architecture` to the project ignores. It does
not add `target`, `node_modules`, or other language-specific paths; put those in
`.complyignore`.

## `[files].exclude`

A rule file may also carry `[files].include` and `[files].exclude`. Their
patterns are relative to the tested directory, not the project root. The rule
file fields are described in [Rules](rules.md).

## Other file rules

- Symlinks are not followed and are not read as files.
- A file that is not valid UTF-8 is skipped and reported in the header.
- Files are sorted by relative path.
- There is no extension filter.
- If the final document is too large, the run is an error. Komply never
  truncates silently.
