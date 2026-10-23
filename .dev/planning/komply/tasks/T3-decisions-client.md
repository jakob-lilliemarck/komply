## Skills

`@repositories`, `@config`, `@documentation` — the project-local skills under
`.opencode/skills/`.

## Goal

Add the `decisions` repository: a self-contained OpenRouter/System One client
that sends one `EvaluationRequest`, validates the `noul` answers, and returns
typed probabilities plus usage. It knows nothing about rules, verdicts, files,
or the CLI.

## Definition of done

- `src/services/testing/repositories/decisions/` exists with `mod.rs`,
  `repository.rs`, and `models.rs`.
- `DecisionsRepository::new` accepts injected credentials, endpoint, timeout, and
  retry policy.
- `DecisionsRepository::from_env` reads `OPENROUTER_API_KEY` and optional
  `COMPLY_DECISIONS_URL`.
- `evaluate` posts one request, retries the allowed failures, validates every
  requested answer, and maps the response into `EvaluationResponse`.
- No service, CLI, or file-selection code changes.
- `cargo test` passes; the tests use a local `TcpListener` stub, never the
  network and never a real key.

## Migrations

None.

## Public API

```rust
// src/services/testing/repositories/decisions/repository.rs — new
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_delay: std::time::Duration,
    pub max_delay: std::time::Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self;
}

pub struct DecisionsRepository {
    // private fields: api key, base URL, timeout, retry policy, agent
}

impl DecisionsRepository {
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        timeout: std::time::Duration,
        retry: RetryPolicy,
    ) -> Self;
    pub fn from_env(
        timeout: std::time::Duration,
        retry: RetryPolicy,
    ) -> Result<Self, Error>;
    pub fn evaluate(
        &self,
        request: &EvaluationRequest,
    ) -> Result<EvaluationResponse, Error>;
}

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

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationResponse {
    pub answers: std::collections::HashMap<String, f64>,
    pub cost_usd: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}
```

`EvaluationRequest` and `EvaluationQuestion` come from `T2` and must not change
shape. `EvaluationRequest::to_json(None)` supplies the body.

## Behavior

### Construction

- `new` stores the key, base URL, timeout, retry policy, and a `ureq::Agent`
  built with the timeout. It reads no environment.
- `from_env` reads `OPENROUTER_API_KEY`; missing or empty is `MissingApiKey`.
  It reads `COMPLY_DECISIONS_URL`; absent or empty means
  `https://openrouter.ai/api/alpha/decisions`. It delegates to a private
  `from_lookup` helper so the lookup is testable.
- `RetryPolicy::default` is five attempts, a one-second initial delay, and a
  thirty-second cap.

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
- Return the validated probabilities as `answers: HashMap<String, f64>`.
- Costs and token counts are optional; a missing `usage` is allowed.

### Retries

- At most `retry.max_attempts` total attempts.
- Retry `429`, every `5xx`, and a `402` whose body contains
  `"limit_source": "openrouter_in_flight_budget"`.
- Do not retry any other `4xx`, including an ordinary `402`.
- Do not retry transport failures; return `Transport`.
- Honour a numeric `Retry-After` header in seconds. Ignore an HTTP-date value.
- Otherwise wait `initial_delay * 2^(attempt - 1)`, capped at `max_delay`, plus
  a jitter of at most 250 ms derived from the system clock.
- On exhaustion return the last `Status`.
- Read the response body before deciding to retry so the error message carries
  it.

## Tests

- A successful request sends `model`, the real `state`, and one `noul` question
  per `EvaluationQuestion`, with the expected `instructions`.
- A successful response maps every answer and the usage fields.
- A missing requested answer is `MissingAnswer`.
- An answer with `type != "noul"` is `MissingAnswer`.
- A probability below `0`, above `1`, or not finite is `InvalidProbability`.
- A `429` followed by `200` retries and succeeds.
- A `500` followed by `200` retries and succeeds.
- A `400` fails immediately with `Status` and the body.
- An in-flight-budget `402` retries.
- An ordinary `402` fails immediately.
- Exhausting the retry budget returns the last status.
- `from_lookup` returns `MissingApiKey` when the key lookup is empty.
- `from_lookup` uses the default URL when the URL lookup is empty and the
  override when it is present.
- `new` does not read the environment.

The stub server reads the request line, headers, and `Content-Length` body, then
writes a JSON response. Use `RetryPolicy { initial_delay: 0, max_delay: 0, .. }`
in retry tests so they stay fast.

## Notes / out of scope

- No threshold interpretation or report construction; T4 owns those.
- No caching, streaming, model fallback, or provider preferences.
- The alpha Decisions endpoint is used; the request and response fields are the
  ones documented for `typesafe/jev-1.13`.
