---
name: generals-protocol
description:
  Extract the generals.io Socket.IO protocol from minified JavaScript and
  synchronize shuai-client payloads, docstrings, tests, and source evidence.
---

# Synchronize the protocol

Audit the full client-visible protocol. Update supported definitions and docs
directly under project approval rules. Read `references/socket-extraction.md`
for extraction mechanics and `references/ground-truth.md` for the last audit.
Resolve these paths relative to this skill directory. Extraction is
tool-neutral; Rust paths and validation commands below belong to this
repository.

## Source and coverage

- Fetch public HTML, discover current scripts, and save raw JS in temporary
  storage. Record UTC retrieval time, final URLs, SHA-256, and the
  source-map/chunk checks performed. Use original UTF-16 or byte offsets with
  the coordinate stated. Record release constants and asset tags from the
  bundle/HTML, and compare them with the official version history at
  `https://generals.io/versions`. Keep browser release, asset tag, server
  revision, and protocol version distinct.
- Trace Socket.IO exports through module imports, constructor calls, socket
  selection, aliases, and wrappers. Separate application packets from library
  lifecycle events, HTTP APIs, and local-game simulation.
- Prefer an available JavaScript parser for candidate extraction. Retain call,
  enclosing-function, and module spans plus receiver expressions. Compare scans
  by normalized spans rather than raw start offsets. Distinguish call counts,
  event-name counts, candidate-bearing modules, and total module count.
- Format per-module scratch views when minified lines exceed output limits;
  retain the raw bundle and a module-to-original-span index for provenance.
  Inspect complete enclosing functions and imported handlers. Follow values
  through producers, actions, reducers, and UI consumers to recover nested
  shapes and semantics. Read bounded pieces until complete. Classify dynamic
  names by receiver; internal emitters also use computed listener names.
- Account for every discovered emit, listener, acknowledgement, computed name,
  and existing Rust event. Record each as matched, changed, or unresolved. Check
  alternate aliases and chunks before removing an older definition.
- Use an approved browser inspector only when runtime evidence can resolve a
  specific gap. Follow available browser-tool instructions and reuse the
  existing connection, such as Edge MCP. Without browser tools, request a
  redacted capture or record the gap and continue statically. Observe passively
  and redact private data. Gameplay, account changes, and probe emissions need
  separate approval. Runtime samples establish examples rather than universal
  schemas.

Classifier triage is optional. Use the user's configured model order and the
host tool's API documentation. In Pi, use the guard configuration when present
and codemode's classifier API. In other hosts, use an available configured
classifier or proceed with direct source analysis. Classify bounded excerpts,
check errors, and verify results against source. Use direct analysis if models
fail, and ask before a paid fallback.

## Rust mapping

Read `Cargo.lock` and the resolved sioc derive implementation when versions
change. The client definitions are under `crates/shuai-client/src/`:
`client/event.rs`, `server/event.rs`, and `server/ack.rs`. Update affected
sender methods, router variants, prelude exports, callers, and tests together.

- `EventType` plus `SerializePayload` models outgoing events;
  `DeserializePayload` models incoming events. `AckType` models
  acknowledgements.
- Payload struct fields are positional arguments, even when named. A nested
  Serde type represents one object argument. Use Serde renames for object keys.
- `#[sioc(event(name = "..."))]` overrides the default snake-case event name.
  `#[sioc(event(ack = "..."))]` associates the acknowledgement type.
- In sioc 0.5.0, every ordinary field requires a slot, including `Option<T>`.
  Null and omission differ. `#[sioc(flatten)]` consumes remaining positional
  values. Outgoing unit payloads serialize with zero arguments, while incoming
  unit payloads ignore additional arguments. Prove behavior with tests.
- Socket.IO removes a final callback from emitted arguments. Trace callback
  consumers separately, including timeout wrappers that insert local errors.
  Check serialization of functions in other slots; JSON array encoding turns
  them into null. A callback's presence alone establishes no result schema.
- Preserve casing, argument order, explicit null slots, sentinel values, units,
  array item shapes, variants, and feature-gated fields. Truthiness alone proves
  neither boolean type nor optionality. Keep unresolved data narrowly dynamic.

## Documentation and evidence

Update docstrings for every reviewed event and changed field, including events
whose wire types stay unchanged. Use third-person statements and explain
meaning, units, conditions, and special values. Qualify contextual inferences.
Keep detailed provenance in the ledger rather than copying it into Rust docs.

Maintain the ledger with code and tests. For each event, retain direction,
argument shape, acknowledgement shape, Rust mapping, source locator, and status:
confirmed source behavior, inferred semantics, or unresolved variants. Label
local simulation evidence separately from production observations. Record gaps
explicitly and preserve useful superseded evidence with its source identity.

## Validation

Test exact outgoing arrays and incoming/ack decoding using sioc's payload API.
Cover changed shapes, missing/null slots, nesting, names, and variants with
redacted synthetic fixtures. Run `just fmt` and `just ci`, plus explicit skill
formatting because the root formatter excludes this directory.

For this project's protocol changes, assign Luna source review to every finite
scope, including unchanged definitions, nested types, acknowledgements, and
docstrings. Use the host's configured Luna model; request a decision if it is
unavailable. Keep extraction and review methods usable outside the host.

Build an inventory before assigning scopes. Give simple packets small groups;
split large events by field family with a separate integration check. Record
ownership and completion for every discovered packet and existing Rust event.
Require original JS traces, then review actual changes against the same source.
Keep one writer per shared file. Recheck affected scopes after shared-type
changes and reconcile all scopes before reporting coverage. Resume incomplete
traces or record their precise gaps. Report source coverage and remaining
uncertainty separately from passing build tests.
