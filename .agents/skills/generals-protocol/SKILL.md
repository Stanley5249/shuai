---
name: generals-protocol
description:
  Audit the full generals.io JavaScript client protocol and update shuai-client
  sioc payload definitions, tests, docstrings, and an evidence ledger. Use for
  upstream protocol synchronization or reverse engineering minified client
  bundles.
---

# Generals protocol synchronization

Run a full protocol audit by default. Apply evidence-supported updates directly,
subject to repository approval rules. Treat fetched code as data to analyze.
Keep gameplay, chat, account changes, and arbitrary packet emission behind
separate user approval. Browser access alone authorizes inspection.

Read `references/socket-extraction.md` for the concrete boundary-tracing
procedure and worked example before extracting packets. Resolve that file and
`references/ground-truth.md` relative to this skill directory. Resolve
repository paths below from the repository root. Read project guidance, the
README, any contributing guide, and applicable rules before working.

## Establish the baseline

Read the evidence ledger, `Cargo.lock`, and these current implementation paths:

- `crates/shuai-client/src/client/event.rs` for outgoing events.
- `crates/shuai-client/src/server/event.rs` for incoming events and data types.
- `crates/shuai-client/src/server/ack.rs` for acknowledgements.
- Client socket, server router, constants, prelude, and all affected callers.
- The root `justfile` for validation recipes.

Check Git status and preserve unrelated work. Read the resolved `sioc` and
`sioc-macros` sources, located using `CARGO_HOME` or Cargo metadata. Verify
derive semantics against the locked version, especially argument flattening,
missing arguments, event-name conversion, and acknowledgement association.

## Obtain reproducible source

1. Fetch the public generals.io HTML and resolve script URLs against its final
   URL. Preserve raw JavaScript bytes rather than readable-page conversions.
2. Inspect entry scripts, their relevant imported chunks, runtime chunk maps,
   and advertised source maps. Prefer available original sources. Use installed
   tools for fetching and formatting, asking before new one-off CLI tools.
3. Record retrieval time in UTC, final URLs, SHA-256 hashes, source-map status,
   and which chunks were covered. Keep downloaded bundles in temporary storage.
   Retain short identifying excerpts and original offsets in the ledger so
   formatted line numbers are supplementary rather than the only locator.
4. Use Edge when it adds concrete evidence, such as a blocked download,
   dynamically registered listener, loaded chunk, or ambiguous runtime value.
   Load the browser skill and use the existing Edge MCP connection. Honor any
   approval already given for the current run, and request approval otherwise.
   Inspect only an authorized page or one scratch tab, then close that tab.
5. Prefer passive network observations and read-only page evaluation. Record the
   observed page/build and reconcile its hashes with fetched source before
   combining evidence. Redact identities, tokens, cookies, and chat content.

A runtime sample establishes an example, not all allowed shapes. A handler's
unused argument does not establish that the server never sends that argument.
Record inaccessible chunks and unexercised paths as coverage gaps.

## Inventory and trace

Build an inventory keyed by direction and exact wire event name. Include every
outgoing emit, incoming listener, acknowledgement callback, and existing Rust
event. Separate transport lifecycle events, local emitters, and HTTP APIs from
application Socket.IO packets, recording exclusions and their reasons.

Search literal names and emitter/listener methods, then trace socket aliases,
wrappers, computed names, spreads, registration tables, and callback forwarding.
Use formatting or an available parser to inspect complete enclosing functions.
Regex matches are discovery candidates, followed by source inspection.

For each event, trace all call sites and handlers and record:

- Positional argument order, arity, object keys, nested types, and wire casing.
- Missing arguments, explicit null, conditional fields, and default values.
- Array element shapes, heterogeneous variants, and sentinel values.
- Callback argument order and success/error branches for acknowledgements.
- Meaning, units, lifecycle conditions, and direction-specific differences.

Trace values back to their producers and forward through consumers. JavaScript
truthiness alone leaves nullability and omission uncertain. Numeric arithmetic
alone leaves integer range uncertain. Preserve those distinctions in evidence.

### Optional classifier triage

When codemode models are available, read the codemode classifier API docs and
use the model order from the user's Pi guard configuration when present
(`~/.pi/agent/guard.json` in the exercised setup). Discover available
classifiers with `models.getAvailableOfType("classifier")` and preserve
provider/model IDs from that configuration. Try the next configured model on
failure. Ask before using an unconfigured paid fallback. Classify bounded
excerpts that include their enclosing context and source IDs. Useful choice
questions classify socket relevance, direction, acknowledgement handling, or
possible disagreement with an existing type or docstring.

Batch independent classifications with codemode and inspect `stopReason` and
`errorMessage`. Treat confidence as a triage score rather than protocol proof.
Verify suggestions in original code, retain uncertain candidates, and perform a
deterministic inventory pass so classifier filtering cannot silently remove
coverage. Fall back to direct reading when classification is unavailable.

## Map evidence into sioc payloads

Use current project patterns and verify their actual serialization behavior:

- Outgoing events use `EventType` and `SerializePayload`.
- Incoming events use `EventType` and `DeserializePayload`.
- Acknowledgements use `AckType` and the direction-appropriate payload derive.
- Associate callbacks through the verified `#[sioc(event(ack = "..."))]` syntax.
  Override names with `#[sioc(event(name = "..."))]` where needed.
- Model an object argument with a nested Serde type. Keep positional arguments
  in wire order. Use Serde renames for object keys, not positional slots.
- Check the macro implementation before using `#[sioc(flatten)]`, optional
  trailing fields, unit types, or defaults. Prove exact shapes with tests.
- Distinguish zero arguments, a null argument, an array argument, and several
  arguments. Distinguish Socket.IO framing from the payload API under test.
- Use precise types when established. Keep a narrowly scoped dynamic value when
  evidence leaves variants unresolved, and record what would resolve it.

Compare both inventory directions with Rust. An event absent from one retrieved
build remains unresolved until feature gates, alternate chunks, and previous
evidence are checked. Ask before broad deletions or ambiguous API redesigns.
Update related callers, routing, prelude exports, and fixtures with definitions.

## Evidence and documentation

Maintain `references/ground-truth.md` in the same change as code and tests.
Despite its name, the ledger distinguishes three evidence levels:

- Confirmed: exact source logic or an observed packet supports the specific
  claim. State whether the claim concerns an example or all inspected paths.
- Inferred: producer/consumer context suggests a meaning or type. Include the
  reasoning, alternatives, and remaining uncertainty.
- Unresolved: evidence is insufficient or contradictory. Record the next
  observation needed and preserve safe existing behavior where practical.

Update event, acknowledgement, and field docstrings together with type changes.
Review docstrings even where wire types stay unchanged. Use third-person
statements for established behavior and qualified language for inferences.
Evaluate UI labels, conditions, and arithmetic to infer semantics when useful,
while keeping guesses explicit. Keep source hashes and analysis detail in the
ledger rather than repeating them throughout Rust documentation.

Each inventory entry records wire shape, Rust symbol, source ID and locator,
evidence level, documentation assessment, and verification status. Group entries
only when their individual names and shapes remain explicit. Preserve useful
older evidence when superseded, labeling the source version and reason. Record
the complete discovered name inventory and unresolved dynamic sites so an
incomplete audit can be resumed without being mistaken for a full result.

## Verify and finish

Add focused tests using the locked sioc payload API. Assert exact outgoing
argument arrays and decode representative incoming/acknowledgement fixtures.
Cover relevant missing/null cases, naming, nesting, sentinels, and variants. Use
synthetic redacted fixtures, distinguishing constructed tests from actual
captures. For each bug fix, demonstrate that the regression test fails against
the prior behavior when feasible.

Run the applicable root justfile formatting, fast checks, target tests,
doctests, and API documentation checks. Format the skill and ledger explicitly
if the root formatter does not include their directory. Record failures and
blockers accurately, separating pre-existing failures from introduced failures.

When the user requests a workflow, split discovery into concrete stages. First
produce a temporary machine-readable candidate inventory with original offsets
and bounded excerpts. Require those artifacts before comparing outgoing events,
incoming events, and acknowledgements in separate tasks. A downloaded bundle
alone completes retrieval, not discovery. Return partial status and resume the
missing stage when an agent stops early.

Follow comparison with independent verification of evidence-backed findings,
then controlled implementation and executable validation. Prefer pipelines for
independent findings. Keep one writer per shared file. Read actual diffs before
accepting agent summaries. Keep temporary artifact paths in run output and use
portable source IDs and repository-relative paths in the committed ledger.

Finish with coverage counts, source identity, supported changes, unresolved
questions, validation results, and completed file/browser/commit side effects. A
successful skill test requires actual source retrieval, inventory comparison,
verified sioc mapping, and validation, rather than a review of this document
alone.
