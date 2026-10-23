---
name: services
description: |
  Use when creating, changing, or reviewing a service. Explains what a service
  is made of and how it is wired up.
---

# Services

This skill describes what the service layer looks like and how it should be written. Apply these patterns whether creating a new service, extending an existing one, or refactoring existing code.

## What a service is

A service **orchestrates business logic**. It coordinates repositories, external API clients, and other services to carry out domain operations. It is the only place business rules are enforced.

**What does NOT go in a service:** content-type conversions, data formatting
for display, extracting previews or summaries for UI presentation, or any
transformation whose purpose is purely presentational. These are not business
logic, regardless of how complex they are. Likewise, HTTP parsing, content
negotiation, and response serialization are not business logic.

**Services are named with verbs** — they describe what they do:
- `billing`, `scheduling`, `rating`, `shipping`, `indexing`

## Public interface conventions

A service's public methods model **business actions**, not data operations. The interface should read like a domain vocabulary, not a CRUD layer.

```rust
// Good — business actions
pub async fn register_user(...) -> Result<User, Error>
pub async fn update_user_profile(...) -> Result<User, Error>
pub async fn start_widget_session(...) -> Result<String, Error>
pub async fn deliver_widget(...) -> Result<String, Error>
pub async fn archive_widget(...) -> Result<Widget, Error>

// Bad — data operations leaking through
pub async fn store_user(...) -> Result<User, Error>
pub async fn get_widget(...) -> Result<Widget, Error>
```

`register_user` and `update_user_profile` are **distinct business operations** even if both ultimately call a repository. The service boundary is where those distinctions live — the repository doesn't need to know.

**Method naming discipline:**
- Name reads like a sentence: `<verb>_<noun>` — `create_widget`, `start_widget_session`, `deliver_widget`
- Each method enforces the business rules that apply to that specific operation
- Method signatures take domain inputs and return domain types, not raw DB rows

## Composing dependencies

A service can hold:

- **Write repositories** — owned exclusively by this service. The write half (`Write`, `WriteTx`) is never accessible outside the owning service. All mutations go through the service. See "Re-exporting nested repositories" below and `@repositories` for visibility.
- **Read repositories** — can come from this service's own aggregates or from **other services' aggregates**. Read repositories are injectable anywhere.
- **External API clients** — held as `Arc<Client>` fields, injected via DI. Live in `crate::apis::`.
- **Nested services** — a service can hold another service as a field and call its public methods directly.

```rust
#[derive(Debug)]
pub struct Service {
    // Owned write repos — private to this service
    widgets_wr: widgets::Write,
    gadgets_wr: gadgets::Write,

    // Read repos — own aggregates
    widgets_ro: widgets::Read,
    gadgets_ro: gadgets::Read,

    // Read repo from another service — allowed, only write is private
    invoices_ro: invoices::Read,

    // External API clients
    some_api: Arc<SomeApiClient>,
    other_api: Arc<OtherApiClient>,

    // Nested service
    other: Arc<other::Service>,
}
```

## Module structure

Every service lives under `src/services/<name>/` and follows this file layout:

| File | Purpose |
|---|---|
| `mod.rs` | Module declarations + `pub use` re-exports |
| `errors.rs` | Error enum |
| `service.rs` | `Service` struct + impl |
| `registrations.rs` | DI provider function + `register()` |
| `events.rs` | Event structs + handlers (when needed — see `@events`) |
| `jobs.rs` | Job messages + handlers (when needed — see `@jobs`) |
| `repositories/` | Aggregate-based repository directories (when needed — see `@repositories`) |

## Visibility rules

| What | Visibility |
|---|---|
| `Service` struct | `pub` |
| Events | `pub` — consumed by other services registering handlers |
| Jobs | `pub(super)` — never dispatched from outside the service |

Repository struct and item visibility live in `@repositories`. The rules for
re-exporting a nested repository module are below.

### Re-exporting nested repositories

The service's `mod.rs` may re-export a repository module so callers can reach
its read side:

```rust
pub use repositories::widgets;
```

This is safe because the repository's `mod.rs` keeps `Write` and `WriteTx` at
service visibility. The service must never widen that:

- Never `pub use repositories::widgets::Write;` or `...::WriteTx;`.
- Never change `Write` or `WriteTx` visibility in the service module.

### `mod.rs`

```rust
mod errors;
pub(crate) mod registrations;
pub(super) mod repositories;
mod service;
// mod events;   // uncomment when events.rs is added
// mod jobs;     // uncomment when jobs.rs is added

pub use errors::Error;
pub use registrations::register;
pub use service::Service;

// Re-export the read side so controllers can use widgets::Read.
// Never re-export Write or WriteTx.
pub use repositories::widgets;
pub use repositories::gadgets;
```

## `errors.rs`

The service error enum **accumulates** `#[from]` conversions for every callee that can fail. Repository errors and API client errors are wrapped transparently. Domain business-rule violations become their own variants.

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // One variant per repository the service calls
    #[error(transparent)]
    WidgetsRepository(#[from] super::repositories::widgets::Error),
    #[error(transparent)]
    GadgetsRepository(#[from] super::repositories::gadgets::Error),

    // Required when the service calls db::begin
    #[error(transparent)]
    Transaction(#[from] crate::infra::db::Error),

    // External API clients
    #[error(transparent)]
    SomeApi(#[from] crate::apis::some_api::Error),

    // Business-rule violations — carry context (e.g. the entity ID)
    #[error("A session already exists for entity {0}")]
    SessionAlreadyExists(Uuid),
    #[error("No session exists for entity {0}")]
    SessionNotFound(Uuid),
}
```

Use `#[from]` for unambiguous conversions. When two source types have the same name, use a manual `From` impl to avoid the conflict.

Add `Transaction(#[from] crate::infra::db::Error)` whenever the service opens a transaction with `db::begin`.

## `service.rs`

### Struct and constructor

```rust
#[derive(Debug)]
pub struct Service {
    widgets_wr: widgets::Write,
    widgets_ro: widgets::Read,
}

impl Service {
    /// Constructs a new `<name>` service.
    #[instrument(level = "debug")]
    pub(super) fn new(widgets_wr: widgets::Write, widgets_ro: widgets::Read) -> Self {
        Self { widgets_wr, widgets_ro }
    }
}
```

`new` is `pub(super)` so only the owning service's `registrations.rs` constructs it. `Write` types are `pub(in super::super)` — accessible from the service because the service sits at the top of the repository's visibility scope.

### Business methods

Public business methods are instrumented at `level = "info"` with `skip(self)`. The constructor and private helpers use `"debug"`. Each method owns its error conversion.

```rust
/// Creates a widget under the given parent.
#[instrument(level = "info", skip(self))]
pub async fn create_widget(
    &self,
    parent_id: Uuid,
    title: String,
    brief: String,
) -> Result<Widget, Error> {
    let now = Utc::now();
    let widget = Widget::new(Uuid::now_v7(), parent_id, title, brief);
    let result = db::begin(self.widgets_wr.clone(), |tx| {
        Box::pin(async move {
            let mut wr = widgets::WriteTx::new(tx);
            let mut rows = wr.store(&[widget], now).await?;
            Ok(rows.pop().unwrap())
        })
    })
    .await?;
    Ok(result)
}
```

### Append-only updates

Mutations are emulated by reading the current state and inserting a new row with a new timestamp:

```rust
/// Updates the widget's title and brief where given.
#[instrument(level = "info", skip(self))]
pub async fn update_widget(
    &self,
    id: Uuid,
    title: Option<String>,
    brief: Option<String>,
) -> Result<Widget, Error> {
    let current = self.widgets_ro.get_by_id(&id).await?;
    let now = Utc::now();
    let updated = Widget::new(
        id,
        current.parent_id(),
        title.unwrap_or_else(|| current.title().to_string()),
        brief.unwrap_or_else(|| current.brief().to_string()),
    );
    let result = db::begin(self.widgets_wr.clone(), |tx| {
        Box::pin(async move {
            let mut wr = widgets::WriteTx::new(tx);
            let mut rows = wr.store(&[updated], now).await?;
            Ok(rows.pop().unwrap())
        })
    })
    .await?;
    Ok(result)
}
```

### `db::begin` closure mechanics

The closure returns `Result<R, anyhow::Error>`. Repository errors inside the closure convert automatically via the blanket `From` impl — `?` works because `anyhow::Error: From<E>` for any `E: std::error::Error + Send + Sync + 'static`. The outer service method converts `db::Error` via `#[from] db::Error`. See `AGENTS.md` → Error Handling.

## `registrations.rs`

```rust
use fx_di::{Container, ProviderResult};
use futures::future::BoxFuture;
use std::sync::Arc;
use tracing::instrument;

/// Constructs the `<name>` service from its repositories.
#[instrument(level = "debug", skip(c))]
fn provide_<name>_service(
    c: &mut Container,
) -> BoxFuture<'_, ProviderResult<Arc<super::Service>>> {
    Box::pin(async {
        let widgets_wr = c.get::<super::widgets::Write>().await?;
        let widgets_ro = c.get::<super::widgets::Read>().await?;
        Ok(Arc::new(super::Service::new(widgets_wr, widgets_ro)))
    })
}

/// Registers the `<name>` service and its sub-repositories.
#[instrument(level = "debug", skip(c))]
pub fn register(c: &mut Container) {
    // Register all sub-repository providers first, then the service.
    c.provide(super::widgets::registrations::provide_widgets_repository_ro);
    c.provide(super::widgets::registrations::provide_widgets_repository_wr);
    c.provide(provide_<name>_service);
}
```

The `provide_*_repository_wr` function has `pub(in super::super::super)` visibility
(visible within this service module), matching the type it returns. Provider
function bodies are documented in `@di`.

To add event or job handlers, register an invokable inside `register()` — the
invokable pattern lives in `@events` and `@jobs`.

## Wiring into `boot.rs`

`src/services/boot.rs` is the DI composition root. Adding a service touches it
twice: register it (and its sub-repositories) in `register_all`, then expose its
read repos in `Repositories` and the service itself in `Services` when an entry
point calls it. The exact per-service checklist lives in `AGENTS.md` → Bootstrap
Wiring; the provider bodies and accessor pattern live in `@di`. Follow those two
— this skill does not repeat them.

## Service-Repository boundary

A service method that merely delegates to a read repository with fixed filter parameters is an **anti-pattern**. Inject the read repository directly into the caller instead. See `AGENTS.md` → Service-Repository Boundary.

```rust
// Bad — service is just a pass-through
impl Service {
    pub async fn list_widgets(&self, parent_id: Uuid) -> Result<Vec<Widget>, Error> {
        let filter = WidgetFilter::default().with_parent_id(parent_id);
        Ok(self.widgets_ro.search(&filter, 50).await?)
    }
}

// Good — controller injects the read repo directly
app.repositories().widgets().search(&filter, 50).await;
```

Service methods should only exist when they:

- Coordinate multiple repositories
- Enforce business rules
- Publish events or dispatch jobs
- Manage transactions

Write repositories remain private to the owning service — mutations always go through the service layer.

## Tests

Test every public business method — its happy path and each way it can fail.
Test a private helper only where the behaviour warrants it. Build the service
through the container with the project's service test harness:

```rust
#[sqlx::test(migrations = false)]
async fn it_creates_the_expected_state(pool: PgPool) -> anyhow::Result<()> {
    crate::migrations::run_default_migrations(&pool).await?;
    let mut c = TestHarness::new(pool.clone()).build().await?;
    let svc = c.get::<Arc<super::Service>>().await?;
    let widget = svc.create_widget(input).await?;
    assert_eq!(widget.id(), expected_id);
    Ok(())
}
```

- **Happy path** — the state after the call is what the rule requires.
- **Validation / business rule** — an illegal input returns the expected error.
- **Transaction rollback** — a failure inside `db::begin` persists nothing.

Whether the method published an event or dispatched a job is tested with the
patterns in `@events` and `@jobs`. Generic rules — no raw SQL, one `seed`
function per module, no AAA comments, no sleeps, human-readable UUIDs — live
in AGENTS.md → Testing Conventions.

## Naming conventions

| Aspect | Convention |
|---|---|
| Service directory | lowercase, underscore-separated **verb** (e.g. `billing`, `scheduling`, `rating`) |
| Service struct | `pub struct Service` |
| Error enum | `pub enum Error` in `errors.rs` |
| Provider fn | `provide_{name}_service` |
| `register()` | `pub fn register(c: &mut Container)` |

## Related skills

- `@repositories` — repository structure, visibility, and query functions.
- `@events` — event types, handlers, publishing, and listener wiring.
- `@jobs` — job messages, handlers, publishing, and listener wiring.
- `@di` — provider functions, module registration, and the composition root.
- `AGENTS.md` — instrumentation tiers, Error Handling, and Bootstrap Wiring.
