## Skills

`@repositories`, `@config`, `@documentation` — the project-local skills under
`.opencode/skills/`.

This task adds no CLI and no service orchestration. It adds the repository's
domain return type.

## Goal

Add the `decisions` repository: a self-contained OpenRouter/System One client
that sends one `EvaluationRequest`, retries transient failures internally,
validates the `noul` answers, and returns a `Decision` with typed probabilities
and usage. It knows nothing about rules, verdicts, files, or the CLI.

## Definition of done

- `src/services/testing/repositories/decisions/` exists with `mod.rs`,
  `errors.rs`, `repository.rs`, and `models.rs`, and re-exports its public items.
- `Decision` exists in `src/services/testing/repositories/decisions/models.rs`
  and carries the answers and usage.
- `DecisionsRepository::new` accepts injected credentials, endpoint, and timeout.
- `DecisionsRepository::from_env` reads `OPENROUTER_API_KEY` and optional
  `COMPLY_DECISIONS_URL`.
- `evaluate` posts one request, retries the allowed failures, validates every
  requested answer, and returns `Decision`.
- No service orchestration, CLI, or file-selection code changes.
- `cargo test` passes; the tests use a local `TcpListener` stub, never the
  network and never a real key.

## Migrations

None.

## Public API

```rust
// src/services/testing/repositories/decisions/models.rs — new
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub answers: std::collections::HashMap<String, f64>,
    pub cost_usd: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}
```

```rust
// src/services/testing/repositories/decisions/repository.rs — new
pub struct DecisionsRepository {
    // private fields: api key, base URL, agent
}

impl DecisionsRepository {
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        timeout: std::time::Duration,
    ) -> Self;
    pub fn from_env(timeout: std::time::Duration) -> Result<Self, Error>;
    pub fn evaluate(
        &self,
        request: &EvaluationRequest,
    ) -> Result<Decision, Error>;
}
```

```rust
// src/services/testing/repositories/decisions/errors.rs — new
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("OPENROUTER_API_KEY is not set")]
    MissingApiKey,
    #[error("decisions request failed: {0}")]
    Transport(String),
    #[error("decisions API returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("could not decode the decisions response: {0}")]
    Decode(String),
    #[error("decisions response is missing a valid `noul` answer for `{0}`")]
    MissingAnswer(String),
    #[error("decisions response has an invalid probability for `{key}`: {value}")]
    InvalidProbability { key: String, value: f64 },
}
```

`EvaluationRequest` and `EvaluationQuestion` come from `T2` and must not change
shape. `EvaluationRequest::to_json(None)` supplies the body.

The retry budget and delays are private constants, not public API and not
config. Tests use a test-only zero-delay setting.

## Behavior

### Construction

- `new` builds a `ureq::Agent` with the timeout and stores the key, base URL,
  and agent. It reads no environment.
- `from_env` reads `OPENROUTER_API_KEY`; missing or empty is `MissingApiKey`.
  It reads `COMPLY_DECISIONS_URL`; absent or empty means
  `https://openrouter.ai/api/alpha/decisions`. It delegates to a private
  `from_lookup` helper so the lookup is testable.

### Request

- Body is `request.to_json(None)`, sent as JSON.
- Headers: `Authorization: Bearer <key>` and `Content-Type: application/json`.
- The repository never sees rule ids; it only sees `EvaluationQuestion.key`
  (`r0`, `r1`, ...) and `instruction`.
- No `criteria` is added.

### Response

- Decode `answers` as a map from key to a typed answer. Decode `usage` with
  optional `cost`, `input_tokens`, and `output_tokens`.
- For every key in `request.questions`, require an answer whose `type` is
  `"noul"` and whose `noul` is a finite number in `[0, 1]`.
- Missing or wrong-type is `MissingAnswer`. A non-finite number or one outside
  `[0, 1]` is `InvalidProbability`.
- Return `Decision { answers, cost_usd, input_tokens, output_tokens }`.
- Costs and token counts are optional; a missing `usage` is allowed.
- Keep the wire response types private. Expose a pure mapping function for
  tests; the transport test and the mapping test are separate.

### Retries

- The repository makes at most four attempts: the first attempt plus three
  retries.
- Retry `429`, every `5xx`, and a `402` whose body contains
  `"limit_source": "openrouter_in_flight_budget"`.
- Do not retry any other `4xx`, including an ordinary `402`, and do not retry
  transport failures, decode errors, missing answers, or invalid probabilities.
- Honour a numeric `Retry-After` header in seconds. Ignore an HTTP-date value.
- Otherwise wait `min(1s * 2^(attempt - 1), 30s)` plus deterministic jitter of
  `(attempt * 37) % 250` ms, where `attempt` is the failed attempt number,
  starting at 1.
- On exhaustion return the last `Status`.
- Read the response body before deciding to retry so the error message carries
  it.
- The budget and delays are private constants. A test-only zero-delay setting
  keeps tests fast; production uses the real values.

## Tests

- A successful request sends `model`, the real `state`, and one `noul` question
  per `EvaluationQuestion`, with the expected `instructions`.
- A successful response maps every answer and the usage fields into `Decision`.
- The wire response mapping is tested separately from the HTTP transport.
- A missing requested answer is `MissingAnswer`.
- An answer with `type != "noul"` is `MissingAnswer`.
- A probability below `0`, above `1`, or not finite is `InvalidProbability`.
- A `429` followed by `200` retries and succeeds.
- A `500` followed by `200` retries and succeeds.
- A `400` fails immediately with `Status` and the body.
- An in-flight-budget `402` retries.
- An ordinary `402` fails immediately.
- A numeric `Retry-After` header is honoured. Use `Retry-After: 0` in the test
  so it stays fast.
- Exhausting the retry budget returns the last status.
- The pure backoff function returns the expected delay for each attempt.
- `from_lookup` returns `MissingApiKey` when the key lookup is empty.
- `from_lookup` uses the default URL when the URL lookup is empty and the
  override when it is present.
- `new` does not read the environment.

The stub server reads the request line, headers, and `Content-Length` body, then
writes a JSON response. Let it accept a sequence of responses so the retry tests
can return `429`, `500`, or `402` before `200`. Tests use the zero-delay setting
so they never sleep.

## Notes / out of scope

- No threshold interpretation or report construction; T4 owns those.
- No caching, streaming, model fallback, or provider preferences.
- The retry budget is fixed and private. Tests use a zero-delay setting so they
  never sleep.
- The alpha Decisions endpoint is used; the request and response fields are the
  ones documented for `typesafe/jev-1.13`.
