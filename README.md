# komply

Komply checks a directory against architectural requirements. It reads rule
files from `.architecture/`, sends the selected files and requirements to Jev
through OpenRouter, and prints a pass / violation / unsure checklist.

The package is `komply`; the installed command is `comply`.

## Install

```sh
cargo install --path .
```

## Quick start

Create a rule file in the project you want to check:

```sh
mkdir -p .architecture
cat > .architecture/repositories.toml <<'EOF'
path = "src/services/*/repositories/*"

[[rules]]
id = "repo.typed-errors"
statement = "A repository defines its own `Error` enum with `thiserror`."
EOF
```

Run the offline preview first. It needs no API key:

```sh
comply test src/services/alpha/repositories/tasks --dry-run
```

Then run the check:

```sh
export OPENROUTER_API_KEY="..."
comply test src/services/alpha/repositories/tasks
```

Every included file is sent to OpenRouter. Use `.complyignore` or
`[files].exclude` to keep secrets out. See [the security note](docs/security.md).

## Documentation

Start at [docs/index.md](docs/index.md):

- [Design](docs/design.md) — how one run works.
- [Usage](docs/usage.md) — commands, output, exit codes.
- [Rules](docs/rules.md) — rule files and path matching.
- [Config](docs/config.md) — thresholds, model, limits.
- [Ignores](docs/ignores.md) — `.complyignore` and `[files]`.
- [Security](docs/security.md) — what leaves your machine.
