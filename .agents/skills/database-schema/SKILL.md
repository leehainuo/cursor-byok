---
name: database-schema
description: Implement and review Cursor BYOK SQLite schema changes. Use when adding or changing tables, columns, indexes, constraints, foreign keys, SQLx migrations, persistence mappings, or database fixtures under server.
---

# Cursor BYOK Database Schema

Treat a schema change as an end-to-end persistence change, not as an isolated SQL edit. Keep the database, Rust store, API contracts, fixtures, and tests aligned.

## Architecture

Use the existing layers and keep responsibilities in their current directories:

```text
server/
├── migrations/              Ordered SQLite/SQLx migrations
├── src/store/               Queries, bindings, row decoding, transactions
├── src/                     Domain and API types that consume persisted data
└── tests/                    Migration, store, API, and integration coverage
```

Before editing, inspect the complete table definition, every query that reads or writes it, its Rust types, API projections, and relevant fixtures. Search by the table name and affected field names; do not infer the persistence path from one file.

## Migration rules

- `server/src/store/sqlite.rs` runs embedded SQLx migrations from `server/migrations`.
- Never modify, rename, reorder, or delete a migration that may already have been applied. SQLx records its checksum; changing an applied file causes startup failure with `migration ... was previously applied but has been modified`.
- Add the next numbered forward migration, using a descriptive filename such as `0003_add_request_protocol.sql`.
- A fresh database must reach the current schema by applying all migrations in order. Do not duplicate a new column or table in both the initial migration and a later migration.
- Only squash or rewrite migration history when the user explicitly asks for a full database reset and accepts that existing databases will no longer start. Do not infer that permission from a development-only workflow.
- Do not add compatibility views, triggers, shadow fields, or fallback reads. Migrate once, then make the application consume the new schema directly.
- Keep one coherent schema change together. Split unrelated changes into separate migrations.

## SQLite design

- Choose nullability from domain meaning. Use `NULL` for genuinely unknown historical data; use a default only when it is correct for every existing row.
- Store booleans as constrained integers, for example `INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1))`.
- Add `CHECK`, `UNIQUE`, and foreign-key constraints when they express real invariants. Choose `ON DELETE` behavior deliberately.
- Add an index only for a demonstrated lookup, join, ordering, or uniqueness requirement. Match its leading columns to actual query shapes.
- Use a transaction for changes that must update multiple tables atomically.
- SQLite supports only limited `ALTER TABLE`. For an unsupported constraint, type, or destructive column change, create the replacement table with the final schema, copy and transform data, replace the old table, and recreate required indexes and foreign keys in one migration.
- Preserve timestamps, identifiers, and existing semantic values during table rebuilds. Do not silently manufacture domain data.

## Application changes

Trace every changed field through the full path that applies:

1. Migration SQL and constraints.
2. Rust domain/request/response structs.
3. SQL column lists, placeholders, `.bind(...)` order, row decoding, and update statements.
4. Transactions and repository/store methods.
5. API serialization and frontend TypeScript types when the field is exposed.
6. UI creation, editing, listing, and details when requested by the product behavior.
7. Test fixtures, literal struct initializers, snapshots, and mock rows.

List SQL columns explicitly. Keep selected-column order, row decoding, insert columns, and bind order visibly aligned. Avoid `SELECT *` because schema additions can silently invalidate positional decoding assumptions.

When a value records the effective behavior of a call, persist the value actually consumed at execution time rather than merely the model or provider default. Keep absent, defaulted, and explicitly supplied values distinguishable when that distinction matters.

## Verification

Add focused coverage proportional to the change:

- A fresh database applies every migration.
- A database at the previous migration upgrades successfully and preserves existing rows.
- Store create/read/update paths round-trip the new fields.
- Defaults, nullability, uniqueness, checks, and foreign keys behave as designed.
- Multi-table writes roll back atomically on failure.
- API and frontend types expose the same semantics when applicable.

Run the narrow tests first, then the repository checks affected by the change. At minimum for server schema work, run:

```bash
cargo fmt --all -- --check
cargo test --workspace
```

If frontend contracts changed, also run from `apps/desktop`:

```bash
npm run check
```

Do not repair unrelated dirty-worktree changes while validating. Report any pre-existing failure separately from failures caused by the schema change.
