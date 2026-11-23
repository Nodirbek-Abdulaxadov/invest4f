# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**Omonat** is an enforcement ledger for Sharia-compliant investment funds (a mudaraba fund that finances murabaha sales). The design is in `plan.md` (in Uzbek); section numbers below (§) refer to it. The plan names .NET + Blazor, but the implementation is **Rust (axum) + React (Vite, TypeScript)**. Code comments, error messages and the UI are in Uzbek. `.vs/` is Karvon IDE state, not project files.

## Commands

```bash
cargo test --workspace                                  # all tests (T1–T13 + unit)
cargo test -p omonat-core --test acceptance t8          # one acceptance test by name
cargo run -p omonat-server                              # API on 127.0.0.1:8787 (8080 is taken on this machine)
OMONAT_DEV=1 cargo run -p omonat-server                 # + /api/dev/* (seed, bank simulator, clock, tamper)

docker compose up -d postgres                           # PostgreSQL 17 on 127.0.0.1:5442 (5433 is taken)
DATABASE_URL=postgres://omonat_app:omonat_app_dev@127.0.0.1:5442/omonat \
DATABASE_OWNER_URL=postgres://omonat_owner:omonat_owner_dev@127.0.0.1:5442/omonat \
OMONAT_DEV=1 cargo run -p omonat-server                 # without DATABASE_URL the ledger lives in memory

# The Postgres store test runs only when these are set (it uses the separate omonat_test DB):
OMONAT_TEST_DATABASE_URL=postgres://omonat_app:omonat_app_dev@127.0.0.1:5442/omonat_test \
OMONAT_TEST_DATABASE_OWNER_URL=postgres://omonat_owner:omonat_owner_dev@127.0.0.1:5442/omonat_test \
cargo test -p omonat-store
```

Web (Node lives in nvm, not on the default PATH: `export PATH=$HOME/.nvm/versions/node/v24.21.0/bin:$PATH`):

```bash
cd web && npm run dev        # Vite on :5173, proxies /api → 127.0.0.1:8787
cd web && npm run build      # tsc --noEmit (TypeScript 7) + vite build → web/dist
```

The server serves `web/dist` (override with `OMONAT_WEB_DIR`) as an SPA fallback. Other env vars: `OMONAT_BIND`, `OMONAT_DATA_DIR` (mock bank state, default `data/`), `OMONAT_RECONCILE_SECS` (default 86400), `OMONAT_HSM_SEED` / `OMONAT_OPERATOR_SEED` / `OMONAT_SHARIA_SEED` (dev keys are derived from fixed seeds when these are unset). For a demo: log in as admin → "Demo ma'lumotlarni yaratish" (`POST /api/dev/seed`).

## Architecture

Workspace crates:

- `omonat-core` — pure domain logic, no IO. All money is `Money(i64)` in **tiyin**; never floats. Pro-rata splits use `money::split_pro_rata` (largest remainder), so sums come out exact.
- `omonat-store` — the `EventStore` trait. `PgStore` is append-only: a trigger blocks UPDATE/DELETE/TRUNCATE even for the table owner, and the `omonat_app` role only has SELECT/INSERT. `MemoryStore` is for tests and dev. The outbox is written in the same transaction as the ledger entries.
- `omonat-bank` — the `Signer` trait (`SoftwareSigner` = Ed25519 standing in for the HSM), `PaymentOrder` / `SignedOrder`, and `MockBank`. The mock bank executes an order only if both the operator and Omonat signatures verify (the 2-of-2 mandate, T12). It is idempotent per payment_id and persists to `data/mock-bank.json` when Postgres is used.
- `omonat-server` — axum API (`api.rs`), the write pipeline and background workers (`app.rs`), dev seed (`seed.rs`).

**Event sourcing.** The ledger is a hash-chained sequence of `Event`s (`event.rs`); `State` (`state.rs`) is built only by applying them. On startup, `Engine::replay` verifies the chain and rebuilds state. If verification fails, the server starts with writes disabled.

**The single write path** is `App::exec`. A command (`Engine::propose_tx`, `approve`, `activate_contract`, …) is a pure function that returns a `Decision { events, outcome }`. Then `Engine::prepare` chains the events into `LedgerEntry`s. For each `TxApplied` payment, the payment order is signed with the operator and Omonat keys and added to the outbox. The store appends the entries and the outbox atomically, and only then does `Engine::apply_entries` update memory. Any effect that needs ids or amounts (new lots, change lots, payments, position deltas) is computed in `compute_effects` and stored inside the event, so replay is deterministic.

**Policy Engine** (`policy/`): each rule implements `PolicyRule { id(); check(tx, &RuleCtx) -> PolicyResult }` and returns `Ok`, `Reject{rule_id, reason}` or `NeedsApproval{rule_id, role}`. The rules are:

- `STRUCT` — structural checks (lots exist and are active, amounts, which actor may propose which kind). The engine stops at the first STRUCT failure.
- `I1`–`I10` — one rule each.
- `FLOW` — a closed-world table of the allowed (kind, source class, destination) flows from §5.

I11 is `Engine::reconcile`, I12 is `chain.rs` plus `verify_anchors`, I13 is `Policy::from_json` / `validate`. Any rejection becomes an `Event::Rejected` in the chain (the rejected log). `NeedsApproval` becomes `TxAwaitingApproval`; once the Sharia board signs, the tx is re-evaluated against the then-current state.

**Proposals.** A `TxProposal` is a list of `Leg { source: Lot | Inflow | Refund, dest: Lot{class, owner} | External{party}, amount }`. Each created lot has exactly one parent, which gives lineage. Change goes back to the parent's owner. `builders.rs` (which includes the Allocation Engine) creates correct proposals, but hand-written proposals (`POST /api/tx`) pass through exactly the same rules. Never let a builder bypass the rules.

**Bank flow.** The outbox worker sends signed orders. `PaymentConfirmed` moves a Deploy contract to `GoodsPurchased`. `PaymentFailed` writes a `Compensation` tx that restores the source lots and cancels the positions. Incoming money is credited at the bank first, then turned into `Deposit` / `Installment` (dev: `/api/dev/bank/incoming`). Reconciliation and outbox processing share the `gateway` lock to avoid false I11 mismatches.

**Contract status.** `Overdue` is derived, not stored: use `Contract::effective_status(local_date(now))`. All dates use Tashkent time (UTC+5) via `omonat_core::local_date`.

## Conventions and gotchas

- To add an invariant: write a `PolicyRule` in `policy/rules.rs`, register it in `default_rules()` (FLOW stays last), and add an acceptance test in `crates/omonat-core/tests/acceptance.rs` that asserts the specific `rule_id`.
- Entry hashes cover the stored payload text and timestamps truncated to microseconds (`chain::normalize_time`), so entries round-trip through Postgres `timestamptz`. Payload is stored as `TEXT`, not `jsonb`. Never re-serialize a payload when verifying.
- Never add a guaranteed or fixed-return field. `Policy` uses `deny_unknown_fields`, `from_json` rejects such keys under I13, and the UI must show expected profit with the "kafolatlanmagan" label.
- Policy parameters (reserve cap, mudarib share, related parties, approved charities) live in the versioned, Sharia-signed `Policy`. Open decisions from §18 (allocation limits, the profit-split order) are not implemented; don't invent values. The current split is: reserve ≤ cap% of profit, mudarib share = % of (profit − reserve), and the rest goes to investors pro-rata by position principal.
- Dev authentication is the `X-Omonat-Actor` header (`operator | sharia | regulator | admin | investor:<uuid>`). The server signs approvals with dev role keys. Both are dev stand-ins for real auth and per-role keys.
- Frontend: API types live in `web/src/api.ts` and mirror the Rust serde shapes (enums are tagged with `type`/`status`/`role`). Money helpers (`som`, `toTiyin`) and Uzbek labels live in `web/src/format.ts`.
