# Config

Project policy lives in `.architecture/config.toml`. A missing file means the
defaults, and keys absent from an existing file keep their defaults. A
malformed file or an invalid value is an error. Unknown keys are rejected.

```toml
[comply]
model = "typesafe/jev-1.13"
pass_threshold = 0.80
fail_threshold = 0.20
max_document_bytes = 400000
request_timeout_seconds = 60
```

| Key | Default | Meaning |
|---|---|---|
| `model` | `typesafe/jev-1.13` | The decisions model. |
| `pass_threshold` | `0.80` | A probability at or above this is a pass. |
| `fail_threshold` | `0.20` | A probability at or below this is a violation. |
| `max_document_bytes` | `400000` | The largest single file and the largest final document. |
| `request_timeout_seconds` | `60` | The HTTP request timeout. |

`noul` is the probability that the attached code satisfies a rule's statement.
A high value is good.

## Validation

- `fail_threshold` must be less than `pass_threshold`.
- Both thresholds must be in `[0, 1]`.
- `max_document_bytes` must be greater than zero.
- `request_timeout_seconds` must be greater than zero.
- Unknown keys, including the old `clean_threshold` and `violation_threshold`,
  are rejected.

## Model override

`--model <id>` replaces `comply.model` for one run. Thresholds, limits, and the
timeout are project policy and have no command-line override.

## Credentials

The config file holds no API key. The key and the endpoint override come from
the environment and are read only when a live run builds the decisions client.
See [Security](security.md) for the variable names, the key handling, and what
leaves your machine.
