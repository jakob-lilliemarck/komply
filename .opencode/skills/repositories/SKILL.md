---
name: repositories
description: |
  Use when reading, writing, creating, refactoring, or extending repository
  layer code. Covers aggregates, models, repository structs, query functions,
  filters, errors, registrations, schema, and migrations. Do NOT use for
  service, controller, or view logic.
---

# Repository Layer

This skill describes how to write a repository module. Apply these patterns
when creating a new repository, extending an existing one, or refactoring one.

These are rules of thumb: they should hold in the overwhelming majority of
cases. A deliberate deviation is allowed when it is justified.

## What is an aggregate?

An aggregate is the tightly coupled set of tables that together represent one
domain concept. One repository manages one aggregate. An aggregate may span
multiple tables: a core table plus child tables that share its identity.

The tables are joined within the repository to produce the complete aggregate.
A missing child is represented by no joined row, not by a nullable column on
the core table. For example, a child row that may not exist is read with a
`LEFT JOIN`, producing `None` instead of a `NULL` column on the core table.

### Aggregate boundaries

Foreign keys and `JOIN`s may be used freely **within** an aggregate, but
**never across** aggregate or repository boundaries. If two repositories need
related data, they pass IDs and look up separately — they do not join across
each other's tables.

- A query may `JOIN` tables owned by the same repository.
- A query must never `JOIN` a table owned by another repository.
- No foreign key references a table in a different aggregate.

## File structure

Each repository lives under `src/services/<service>/repositories/<aggregate>/`:

```
repositories/<aggregate>/
├── mod.rs
├── errors.rs
├── repository.rs
├── registrations.rs
├── models.rs
└── queries/
    ├── mod.rs
    ├── search_{entity}.rs
    ├── store_{entity}.rs
    └── archive_{entity}.rs (optional — for entities that support deletion)
```

- `<aggregate>` is plural, lowercase, underscore-separated (e.g. `widgets`).
- `<aggregate>` must be a noun. Use `widgets`, not `process_widgets`.
- `<aggregate>` names the domain concept, not a specific table.
- Query files are named `{verb}_{entity}`. The entity is the aggregate for the
  common case (`search_widgets`), or a child or projection when the query reads a
  different view (`search_widget_sessions`, `store_widget_sessions`).

## Models (`models.rs`)

Declare the module as `pub mod models` in `mod.rs` so callers can reach the
domain types.

```rust
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct {Singular} {
    pub(super) id: Uuid,
    // ... domain fields ...
}

impl {Singular} {
    /// Creates a new entity.
    #[instrument(level = "debug")]
    pub fn new(id: Uuid) -> Self {
        Self { id }
    }

    /// Returns the entity's identifier.
    #[instrument(level = "debug")]
    pub fn id(&self) -> Uuid {
        self.id
    }
}
```

- Fields are `pub(super)`; accessors are `pub`.
- External callers construct entities through `pub fn new(...)`.
- Use `Uuid::now_v7()` for IDs.
- A domain model never carries `timestamp` or `archived`.
- The model derives `sqlx::FromRow` because it is the default
  `build_query_as` target. When a `*Storage` type is the insert target, derive
  it there instead. Reads via `query_as!` need no trait impl.

### Storage row types

By default a query reads straight into the domain model. `timestamp` and
`archived` are storage-only and are never selected:

- `archived` appears only in SQL predicates such as
  `WHERE NOT latest.archived`.
- `timestamp` appears only in SQL ordering (`ORDER BY timestamp DESC`) and as
  the insert bind.

```rust
// No storage type: the query builds the model directly.
let rows = sqlx::query_as!(
    Singular,
    r#"
    SELECT id, name
    FROM (
        SELECT DISTINCT ON (id) *
        FROM schema.{aggregate}
        ORDER BY id ASC, timestamp DESC
    ) latest
    WHERE NOT latest.archived
    "#,
)
.fetch_all(tx)
.await?;
```

Introduce a `pub(crate) {Singular}Storage` row type only when a selected column
needs a mapping the model cannot express. The usual case is a `TEXT` column
that holds a fixed set of values, represented in code as an enum:

```rust
#[derive(Debug, Clone)]
pub(crate) struct {Singular}Storage {
    pub(super) id: Uuid,
    pub(super) kind: String, // raw column value
    // ... remaining raw columns ...
}

impl TryFrom<{Singular}Storage> for {Singular} {
    type Error = Error;

    fn try_from(row: {Singular}Storage) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            kind: Kind::try_from(row.kind.as_str())?,
        })
    }
}
```

- When a `*Storage` type is used, the query method strips it to the domain type
  before data leaves the method; the caller never sees the `*Storage` type:
  ```rust
  Ok(rows
      .into_iter()
      .map(Singular::try_from)
      .collect::<Result<Vec<_>, _>>()?)
  ```
- Do not defer the conversion to a facade or to the caller.
- `archive_*` needs no `*Storage` type: it hardcodes `archived = true`, takes
  one timestamp argument, and returns domain columns directly.
- `store_*` uses `QueryBuilder::build_query_as`, whose target derives
  `sqlx::FromRow` — the model by default, the `*Storage` type when a mapping
  exists.
- If the aggregate does not support deletion, omit `archived` from the table
  and its queries.

## Repository structs (`repository.rs`)

Three structs exist: `Read`, `Write`, and `WriteTx`. `Read` and `WriteTx` are
required. `Write` is optional — add it only for standalone, one-off writes made
outside a transaction. Transactional mutations go through `WriteTx`.

```rust
use crate::infra::db;
use sqlx::{PgConnection, PgTransaction};

#[derive(Debug, Clone)]
pub struct Read {
    ro: db::ReadPool,
}

#[derive(Debug, Clone)]
pub(in super::super::super) struct Write {
    wr: db::WritePool,
}

pub(in super::super::super) struct WriteTx<'tx> {
    tx: &'tx mut PgTransaction<'static>,
}
```

`Write` implements `db::Tx`:

```rust
impl db::Tx for Write {
    type Error = sqlx::Error;

    fn tx(self) -> db::TxFut<Self::Error> {
        Box::pin(async move { self.wr.pool.begin().await })
    }
}
```

`Read` is `pub` and injectable into any caller, including controllers. Only
`Write` and `WriteTx` are service-private. They use relative-path visibility so
the owning service is never hardcoded:

| Location | Visibility | Resolves to |
|---|---|---|
| `mod.rs` — re-export | `pub(in super::super)` | 2 hops from `repositories::<aggregate>` reaches the service |
| `repository.rs` — struct def | `pub(in super::super::super)` | 3 hops from `repositories::<aggregate>::repository` reaches the service |
| `registrations.rs` — `provide_wr` | `pub(in super::super::super)` | matches the returned `Write`/`WriteTx` type |

How the owning service re-exports its nested repository module is a service
concern; see `@services`. The repository only defines the item visibilities
above and never widens them.

Callers reach public items through the aggregate `mod.rs`, re-exported as
`{aggregate}::{Type}`. Re-export what callers use:

```rust
mod errors;
pub mod models;
mod queries;
mod repository;

pub mod registrations;

pub use errors::Error;
pub use models::Singular;
pub use queries::search_{entity}::{Entity}Filter;
pub use repository::Read;

pub(in super::super) use repository::Write;
pub(in super::super) use repository::WriteTx;
```

`queries` stays private. The `#[cfg(test)] pub(crate) use` re-exports of query
functions belong here when tests need them.

### Facades

A facade is a thin wrapper over one or more query functions. Add one whenever
it makes the calling code clearer. Facades are not tested; test the underlying
query functions.

### Read methods

`Read` exposes facade methods over the query functions. The main facade is
`search`, backed by `search_{entity}`. Add another facade only when a caller
reads a different view of the aggregate. Expose a method only when a caller
uses it.

Facade methods may be `pub` when other services or controllers use them, or
`pub(crate)` when only the owning service does.

`get_by_id` is optional and, when exposed, wraps the search with limit 1:

```rust
impl Read {
    /// Creates a new read-only repository.
    #[instrument(level = "debug")]
    pub fn new(ro: db::ReadPool) -> Self {
        Self { ro }
    }

    /// Searches the aggregate with the given filter and limit.
    #[instrument(level = "debug", skip(self))]
    pub async fn search(
        &self,
        filter: &{Entity}Filter,
        limit: i64,
    ) -> Result<Vec<{Singular}>, Error> {
        super::queries::search_{entity}(&self.ro.pool, filter, limit).await
    }

    /// Fetches a single entity by ID. Returns `NotFound` when absent.
    #[instrument(level = "debug", skip(self))]
    pub async fn get_by_id(&self, id: &Uuid) -> Result<{Singular}, Error> {
        let filter = {Entity}Filter::default().with_ids(vec![*id]);
        let mut results =
            super::queries::search_{entity}(&self.ro.pool, &filter, 1).await?;
        results.pop().ok_or_else(|| Error::NotFound(id.to_string()))
    }
}
```

### WriteTx methods

`WriteTx` holds the transaction and exposes `pub(crate)` insert-only facade
methods. There are no update or delete methods: mutations append a new row with
a new timestamp.

```rust
impl<'tx> WriteTx<'tx> {
    /// Creates a new transactional write repository.
    #[instrument(level = "debug")]
    pub fn new(tx: &'tx mut PgTransaction<'static>) -> Self {
        Self { tx }
    }

    /// Stores a batch of entities under the given timestamp.
    #[instrument(level = "debug", skip(self, items))]
    pub(crate) async fn store_{entity}<'a, I>(
        &mut self,
        items: I,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<{Singular}>, Error>
    where
        I: IntoIterator<Item = &'a {Singular}>,
    {
        let conn: &mut PgConnection = &mut *self.tx;
        super::queries::store_{entity}(conn, items, timestamp).await
    }

    /// Archives every latest, non-archived entity matching the filter.
    #[instrument(level = "debug", skip(self))]
    pub(crate) async fn archive_{entity}(
        &mut self,
        filter: &{Entity}Filter,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<{Singular}>, Error> {
        let conn: &mut PgConnection = &mut *self.tx;
        super::queries::archive_{entity}(conn, filter, timestamp).await
    }
}
```

`&mut *self.tx` must be cast explicitly to `&mut PgConnection`.
`PgTransaction` derefs to `PgConnection`, and `&mut PgConnection` implements
`PgExecutor`.

**Chunking large batches.** A multi-row `VALUES` insert binds one parameter per
column per row, and Postgres rejects a statement with more than 65,535
parameters. When `store` can receive a caller-sized, unbounded batch,
`WriteTx::store` chunks it — never the query function, and never by a fixed row
count:

```rust
let rows = db::rows_per_statement(super::queries::store_{entity}::BINDS_PER_ROW);
let mut stored = Vec::with_capacity(items.len());
for chunk in items.chunks(rows) {
    let conn: &mut PgConnection = &mut *self.tx;
    stored.extend(super::queries::store_{entity}(conn, chunk.iter(), timestamp).await?);
}
Ok(stored)
```

The query declares `pub(crate) const BINDS_PER_ROW` beside the SQL so the count
cannot drift; `db::rows_per_statement` derives the safe row budget from
`MAX_BIND_PARAMS`. Every chunk runs on the caller's transaction, so the batch
stays atomic. Bounded stores — small per-request batches, or batches already
chunked by the service — keep the plain single-statement `store`.

### Write methods

`Write` is the standalone counterpart. It is not inside a transaction, so it
may compute its own timestamp:

```rust
impl Write {
    /// Creates a new write repository.
    #[instrument(level = "debug")]
    pub fn new(wr: db::WritePool) -> Self {
        Self { wr }
    }

    /// Stores a batch of entities as a standalone write.
    #[instrument(level = "debug", skip(self, items))]
    pub async fn store_{entity}<'a, I>(
        &self,
        items: I,
    ) -> Result<Vec<{Singular}>, Error>
    where
        I: IntoIterator<Item = &'a {Singular}>,
    {
        let timestamp = Utc::now();
        super::queries::store_{entity}(&self.wr.pool, items, timestamp).await
    }
}
```

## Timestamps

- `queries::store_{entity}` and `queries::archive_{entity}` always take
  `timestamp: DateTime<Utc>` and never call `Utc::now()`.
- `WriteTx` facade methods always take `timestamp: DateTime<Utc>` and pass it
  through, so every step of one transaction shares one timestamp.
- `Write` facade methods may call `Utc::now()` themselves. A standalone write
  is ad-hoc and gets its own timestamp.

## Query functions (`queries/`)

**Three query functions per entity, as a rule of thumb.** An aggregate normally
needs exactly `search_{entity}`, `store_{entity}`, and `archive_{entity}` — one
file each; extend the filter and reuse `search` before adding another. A genuine
exception (counts, existence, similarity, locking) is allowed and carries a
docstring saying why the three do not cover it.

**One query function per file.** Never put multiple query functions in one
file. Register each file in `queries/mod.rs`.

The `queries` module is private (`mod queries`). `queries/mod.rs` declares its
files `pub(crate)` and re-exports the model, plus any storage type the query
files use:

```rust
pub(crate) mod archive_{entity};
pub(crate) mod search_{entity};
pub(crate) mod store_{entity};

pub use super::models::{Singular};
// Only when a query needs a custom mapping.
pub(crate) use super::models::{Singular}Storage;
```

### `search_{entity}.rs` — filtered query

The "latest version per ID" logic is handled inline with a `DISTINCT ON`
subquery. There is no database view. The outer query filters on the latest
version, so `WHERE NOT latest.archived` excludes a record whose newest row is
archived even when older rows are not.

The filter for a search lives in the same file and is named `{Entity}Filter`:

```rust
use super::Singular;
use super::super::errors::Error;
use sqlx::PgExecutor;
use tracing::instrument;
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct {Entity}Filter {
    pub(super) ids: Option<Vec<Uuid>>,
    pub(super) cursor: Option<Uuid>,
}

impl {Entity}Filter {
    /// Filters by one or more entity IDs.
    #[instrument(level = "debug")]
    pub fn with_ids(mut self, ids: Vec<Uuid>) -> Self {
        self.ids = Some(ids);
        self
    }

    /// Starts after the given ID.
    #[instrument(level = "debug")]
    pub fn with_cursor(mut self, cursor: Uuid) -> Self {
        self.cursor = Some(cursor);
        self
    }
}

/// Searches the aggregate with the given filter and limit.
#[instrument(level = "debug", skip(tx))]
pub async fn search_{entity}<'tx, E: PgExecutor<'tx>>(
    tx: E,
    filter: &{Entity}Filter,
    limit: i64,
) -> Result<Vec<Singular>, Error> {
    let rows = sqlx::query_as!(
        Singular,
        r#"
        SELECT
            latest.id,
            latest.name
        FROM (
            SELECT DISTINCT ON (id) *
            FROM schema.{aggregate}
            ORDER BY id ASC, timestamp DESC
        ) latest
        WHERE NOT latest.archived
          AND ($1::uuid[] IS NULL OR latest.id = ANY($1))
          AND ($2::uuid IS NULL OR latest.id > $2)
        ORDER BY latest.id ASC
        LIMIT $3
        "#,
        filter.ids as _,
        filter.cursor as _,
        limit,
    )
    .fetch_all(tx)
    .await?;
    Ok(rows)
}
```

Key points:

- The `DISTINCT ON (id) ... ORDER BY id ASC, timestamp DESC` subquery selects
  the latest row per ID.
- `WHERE NOT latest.archived` filters the latest version only.
- Select only domain columns. `timestamp` stays in the subquery's `ORDER BY`
  and `archived` in the `WHERE`; neither is returned.
- A filter may be pushed inside the subquery only when its column is immutable
  — the same value in every stored version of an ID. `id` and the keyset
  cursor qualify, as do creation-fixed columns. Mutable columns (`archived`,
  `title`, `name`, any field that can change between versions) stay in the
  outer query, or an older version becomes the latest and resurfaces.
- Optional filters use the `$1 IS NULL OR column = $1` pattern.
- Forward keyset pagination uses `id > $2` with `ORDER BY id ASC`. Never use
  `OFFSET`.
- A filter is named `{Entity}Filter`, naming what is searched — `WidgetFilter`,
  `WidgetSessionFilter`, `OrderFilter`. Never name it `Filter`: re-exporting a
  bare `Filter` from `mod.rs` collides across queries, and this project does not
  use import aliases.
- Filters are `pub`; their fields are `pub(super)` so the aggregate's query
  functions can read them, and callers set them through `with_*`.
- Re-export from `mod.rs` only the filters callers actually use.

### `store_{entity}.rs` — batch insert

Always batch. Callers provide the timestamp so all rows in one operation share
the same value. The insert targets the model by default; when a `*Storage` type
exists, target it and map back.

```rust
use super::Singular;
use super::super::errors::Error;
use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, QueryBuilder};
use tracing::instrument;

/// Batch-inserts entities into the table.
#[instrument(level = "debug", skip(tx, items))]
pub(crate) async fn store_{entity}<'tx, 'a, I, E>(
    tx: E,
    items: I,
    timestamp: DateTime<Utc>,
) -> Result<Vec<Singular>, Error>
where
    E: PgExecutor<'tx>,
    I: IntoIterator<Item = &'a Singular>,
{
    let mut builder = QueryBuilder::new(
        "INSERT INTO schema.{aggregate} (id, timestamp, archived, name) ",
    );

    builder.push_values(items, |mut b, item| {
        b.push_bind(item.id());
        b.push_bind(timestamp);
        b.push_bind(false);
        b.push_bind(item.name());
    });

    builder.push(" RETURNING id, name");

    let rows = builder
        .build_query_as::<Singular>()
        .fetch_all(tx)
        .await?;
    Ok(rows)
}
```

`build_query_as` requires `FromRow` on its target. Omit `archived` when the
aggregate does not support deletion.

When a store can receive an unbounded batch, declare
`pub(crate) const BINDS_PER_ROW` beside the SQL (see "Chunking large batches").

### `archive_{entity}.rs` — CTE-based deletion

For aggregates that support deletion, `archive` takes the **same filter as
`search`** and appends a row with `archived = true` for every latest,
non-archived row matching it. A plain delete sets only `ids` on the filter.
Callers provide the timestamp.

```rust
use super::search_{entity}::{Entity}Filter;
use super::Singular;
use super::super::errors::Error;
use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use tracing::instrument;

/// Archives every latest, non-archived entity matching the filter.
#[instrument(level = "debug", skip(tx))]
pub(crate) async fn archive_{entity}<'tx, E: PgExecutor<'tx>>(
    tx: E,
    filter: &{Entity}Filter,
    timestamp: DateTime<Utc>,
) -> Result<Vec<Singular>, Error> {
    let rows = sqlx::query_as!(
        Singular,
        r#"
        WITH latest AS (
            SELECT DISTINCT ON (id) *
            FROM schema.{aggregate}
            WHERE ($1::uuid[] IS NULL OR id = ANY($1))
            ORDER BY id ASC, timestamp DESC
        ),
        to_archive AS (
            SELECT id, name
            FROM latest
            WHERE NOT archived
        ),
        inserted AS (
            INSERT INTO schema.{aggregate} (id, timestamp, archived, name)
            SELECT id, $2::timestamptz, true, name
            FROM to_archive
            RETURNING id, name
        )
        SELECT id, name
        FROM inserted
        "#,
        filter.ids as _,
        timestamp,
    )
    .fetch_all(tx)
    .await?;

    Ok(rows)
}
```

Key points:

- `latest` evaluates before the insert, so the CTE sees the pre-archive state.
- Push the filter's **immutable identity** predicates into `latest` — the same
  ones `search_{entity}` uses. `WHERE NOT archived` in `to_archive` makes the
  operation idempotent.
- Archive is not a search: it must not inherit read-time constraints such as
  expiry. An expired record is still archived.
- A plain delete builds the filter with only ids — for example
  `{Entity}Filter::default().with_ids(ids)`. The `WriteTx` facade takes the
  filter, not a raw id list.

## Errors (`errors.rs`)

Every repository defines its own error enum with `thiserror`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Resource not found: {0}")]
    NotFound(String),
}
```

Add domain variants such as `Conflict` where the repository can detect them.
Map `sqlx::Error::RowNotFound` and empty results to `Error::NotFound`, not to
`Error::Database`.

## Registrations (`registrations.rs`)

Every repository has a `registrations.rs` with one provider function per
repository struct:

- `provide_{aggregate}_repository_ro` — returns `Read`, `pub`.
- `provide_{aggregate}_repository_wr` — returns `Write`,
  `pub(in super::super::super)`.
- Additional providers for any other exported structs.

Do not add a helper `register` method to a repository. The owning service
registers the providers individually. Provider function bodies are documented
in `@di`.

## Schema and migrations

Each aggregate has a base table only; there is no database view.

**`.up.sql`:**

```sql
CREATE TABLE schema.{aggregate}_table (
    id UUID NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    -- archived BOOLEAN NOT NULL, -- only when the aggregate supports deletion
    -- ... domain columns ...
);
```

**`.down.sql`:**

```sql
DROP TABLE IF EXISTS schema.{aggregate}_table CASCADE;
```

- No `DEFAULT` values. The database is a dumb store; every column, including
  `timestamp`, is provided explicitly on insert.
- Prefer `NOT NULL`. When data arrives later, split it into a second table
  rather than adding a nullable column.
- Use `UUID` primary keys. Composite keys are acceptable; never `SERIAL` or
  `INTEGER`. IDs come from the application.
- Schema-qualify every table reference in migrations and in `query!` /
  `query_as!`, for example `schema.{aggregate}`.
- Add `archived BOOLEAN NOT NULL` only when the aggregate supports deletion.
- To change an existing aggregate, add a new migration pair
  (`{YYYYMMDDHHMMSS}_{name}.up.sql` / `.down.sql`) with `ALTER TABLE`. Never
  edit an existing migration.

After adding a migration or changing a query macro, run `run_migrations up` →
`refresh_query_cache` → `run_tests`, then commit `.sqlx/`. See `AGENTS.md` →
Query Cache & Migrations Workflow.

## Query rules

- Prefer `sqlx::query_as!` and `sqlx::query!` for compile-time verification. If
  a query cannot be statically checked, change it so it can or split it. A
  runtime `query_as`/`query` is a last resort and must carry a comment saying
  why.
- Use `RETURNING` on every insert.
- Format SQL for readability: clause keywords on new lines, indent columns.
- Pass non-`Copy` arguments by reference; clone inside the function if needed.
- Always require `LIMIT` on collection queries.
- Repositories do not validate. Validation happens at boundaries; the database
  and `sqlx` enforce structural integrity on read.
- `"column!:Type"` overrides are only for a mismatch between what sqlx infers
  and the Rust field:
  - **`LEFT JOIN` columns** — inferred as nullable (`Option<T>`).
  - **Computed or aggregate columns** — `ARRAY_AGG`, `COUNT`, and the like.
  - **Any column sqlx infers as `Option<T>` while the Rust field is `T`.**
  Plain `NOT NULL` columns from a `DISTINCT ON` subquery resolve correctly
  without `!`. Do not add an override that is not needed.

## Tests

Every query function is tested exhaustively — `search_*`, `store_*`,
`archive_*`, and any custom query such as `count_*` or `summary_*`. What
"exhaustive" means depends on the function: cover every branch, filter,
boundary, and distinct outcome it can produce. The point is coverage of the
function's behaviour, not a fixed checklist of cases.

`Read` and `WriteTx` methods are thin wrappers over a query function and add no
behaviour of their own, so a fully tested query function needs no facade test.

```rust
#[cfg(test)]
mod tests_search_{entity} {
    use super::{{Entity}Filter, search_{entity}};

    async fn seed(pool: &PgPool) -> anyhow::Result<Vec<Uuid>> {
        let now = Utc::now();
        let stored = store_{entity}(pool, &[a, b], now).await?;
        Ok(stored.iter().map(|row| row.id()).collect())
    }

    #[sqlx::test(migrations = false)]
    async fn it_returns_only_latest_version(pool: PgPool) -> anyhow::Result<()> {
        crate::migrations::run_default_migrations(&pool).await?;
        let ids = seed(&pool).await?;
        store_{entity}(&pool, &[with_title(ids[0], "changed")], Utc::now()).await?;
        let rows = search_{entity}(&pool, &{Entity}Filter::default(), 10).await?;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title(), "changed");
        Ok(())
    }
}
```

Tests live in a `#[cfg(test)] mod` in the query file. `#[sqlx::test(migrations = false)]`
gives a fresh database per test, and `run_default_migrations` runs first.
Assert exact IDs and values from the seed, never vague bounds. Generic rules —
no raw SQL, one `seed` function per module, no AAA comments, no sleeps,
human-readable UUIDs — live in AGENTS.md → Testing Conventions.

## Naming conventions

| Aspect | Convention |
|---|---|
| Directory | lowercase, plural, underscore-separated noun (e.g. `widgets`) |
| Error enum | `pub enum Error` with `Database`, `NotFound`, and domain variants |
| Repository structs | `Read` and `WriteTx` required; `Write` optional for standalone writes |
| Provider functions | `provide_{aggregate}_repository_ro` always; `provide_{aggregate}_repository_wr` when `Write` exists |
| `Read` visibility | `pub` |
| `Write` / `WriteTx` visibility | `pub(in super::super)` from `mod.rs`; `pub(in super::super::super)` from `repository.rs` and `registrations.rs` |
| `queries` module | private; `queries/mod.rs` declares files `pub(crate)` and re-exports the model, plus storage when used |
| Filter | `{Entity}Filter`, defined in its search file and re-exported from `mod.rs` when used |
| `search_{entity}` | `pub` query function — one per file |
| `store_{entity}` | `pub(crate)` query function — batch insert only |
| `archive_{entity}` | `pub(crate)` query function — CTE insert with `archived = true` |
| Domain model fields | `pub(super)` with `pub` accessor methods |
| Storage row type | `{Singular}Storage`, `pub(crate)` — only when a selected column needs a custom mapping; converted inside the query method |
| `FromRow` | on the `build_query_as` target (the model by default, `*Storage` when a mapping exists); `query_as!` reads need none |
| Mutations | Append-only — new rows with new timestamps; no `UPDATE` or `DELETE` |

## Related skills

- `@di` — provider functions and DI registration.
- `@services` — service boundaries, transactional outbox, jobs.
- `AGENTS.md` — Query Cache & Migrations Workflow.
