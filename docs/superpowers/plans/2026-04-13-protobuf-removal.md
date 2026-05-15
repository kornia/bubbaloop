# Protobuf Removal — CBOR-only (2026-04-13)

**Branch:** `feat/remove-protobuf`
**Strategy:** CBOR is self-describing (field names live as map keys). No schema crate, no schema queryables, no descriptor registry. Nodes own types as plain serde structs; consumers `cborDecode` and treat the result as a generic object.

## Phases

### Phase 1 — SDK cleanup (Rust + Python)

Remove all proto surface from `crates/bubbaloop-node/` and `python-sdk/`. Keep CBOR + raw + JSON only.

Rust: delete `ProtoPublisher`, `TypedSubscriber`, `proto_decoder.rs`, `schema.rs` (schema queryable), `publisher_proto*`, `subscriber()` typed variant. Drop `prost`, `prost-types`, `bubbaloop-schemas` deps.
Python: delete `ProtoSubscriber`, `ProtoPublisher`, `ProtoDecoder`, schema fetcher. Add `CborPublisher`/`CborSubscriber`. Drop `protobuf` dep.
Delete whole crate: `crates/bubbaloop-node-build/`.

Verify: `cargo check -p bubbaloop-node`, Python import smoke.

### Phase 2 — Daemon wire → CBOR

`daemon/nodes`, `daemon/events`, other proto-emitting Zenoh topics → CBOR serde structs.
Dashboard: remove `decodeNodeList`/`decodeNodeEvent` proto fallbacks.

Verify: `pixi run check`, daemon runs, dashboard node list populates.

### Phase 3 — Nodes (parallel per node)

For each node in `bubbaloop-nodes-official/`: delete `protos/`, `build.rs`, build-deps. Replace generated types with serde structs. Switch publishers to `publisher_cbor`.

Verify: each node starts, data reaches dashboard.

### Phase 4 — Dashboard

Delete `src/lib/schema-registry.ts`, `src/proto/`, `SchemaRegistryContext`, `useSchemaReady`, `discoverForTopic`. JsonView chain becomes: encoding=CBOR → `cborDecode`; encoding=JSON → `JSON.parse`; else hex. Remove `vendor-proto` chunk, uninstall `protobufjs`, `long`, `scripts/generate-proto.sh`.

Verify: `npm run build && npm test`, dashboard renders camera + raw panels.

### Phase 5 — Delete proto infrastructure (point of no return)

Delete `crates/bubbaloop-schemas/`, `crates/bubbaloop-node-build/`, `descriptor.bin`, `DESCRIPTOR` const. Drop `prost*`, `protoc` from CI, `protobufjs` from dev docs.

Verify: `pixi run test && pixi run clippy && pixi run fmt-check`.

### Phase 6 — Templates + docs

Update `templates/rust-node/`, `templates/python-node/`, `ARCHITECTURE.md`, `CLAUDE.md`, `CONTRIBUTING.md`, READMEs. Drop proto checks from `scripts/validate.sh`.

### Phase 7 — Release

Changelog, migration note for out-of-tree nodes, version bump.

## Parallelization

- Phase 1 Rust vs Python: parallel.
- Phase 3: one executor per node.
- Phase 6: split docs / templates.

## Rollback

One commit per phase. Revert on smoke-test failure.
