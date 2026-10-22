# Extracting the application Socket.IO client

Use this procedure to recover the application's socket boundary, then inventory
its packets. Extracting the boundary means tracing constructors, aliases,
wrappers, and consumers. Rebuilding or executing the entire bundle is usually
unnecessary.

## Preserve searchable source

Save raw response bytes, compute SHA-256, and decode UTF-8 for analysis. Record
both byte offsets and JavaScript UTF-16 string offsets when producing snippets.
They differ for non-ASCII text. Use bounded excerpts rather than dumping a
multi-megabyte minified line into a tool response. Continue paginated reads of
artifacts until coverage is complete.

Formatting improves readability but changes positions. Retain the original
source and locate findings with its hash, original offset, and short excerpt.
Use source maps when advertised and accessible. Inspect bundler module tables,
chunk-loader functions, script creation, and dynamic imports for additional
application code. A failed search for one bundler's chunk syntax establishes
only that the searched pattern was absent.

## Find the transport and application boundary

1. Search stable library strings such as `socket.io-client`,
   `socket.io-client:manager`, `engine.io-client`, `/socket.io`,
   `connect_error`, `receiveBuffer`, and `acks`. These are anchors, not
   sufficient proof alone.
2. Identify the enclosing module and exported constructor or lookup function. In
   a numeric module table, follow the module ID back to import sites such as
   `n(8007).io`. In other bundlers, follow equivalent import/export bindings.
3. Follow constructor calls to endpoint expressions and options. Resolve
   hostname branches, namespaces, custom paths, and socket-selection logic.
   Different deployment hosts may select different servers.
4. Trace assignments, returned objects, exported bindings, and wrapper
   parameters to find the selected application socket. Respect lexical scope. A
   minifier can reuse the same short variable name in unrelated modules.
5. Follow wrappers such as debounce functions and `.emit.apply(...)`, bound
   methods, bracket access, spread arguments, and forwarding callbacks. Trace
   imported listener functions into their defining module.
6. Classify each emitter by receiver identity. Socket.IO's local lifecycle
   events, Engine.IO transport packets, UI event buses, and application wire
   events are distinct inventories.

A regular expression provides candidate sites, while source inspection or an
available parser establishes receiver identity and argument boundaries. Commas
inside nested calls, objects, arrays, and functions require syntax-aware
handling. Retain computed event names as unresolved sites until their value sets
can be established.

## Recover packet shapes

For an application call `socket.emit(name, a, b, callback)`, verify the bundled
implementation's treatment of the final function. Ordinary Socket.IO emission
registers the callback as an acknowledgement handler and sends the event name
and data arguments separately from the callback. The callback's parameters
therefore describe the acknowledgement, not outgoing payload fields.

Inspect timeout and promise-based wrappers separately. A timeout wrapper may
prepend a local error argument, which is distinct from server acknowledgement
data. Trace listener forwarding and callback invocation before assigning arity.

Keep these layers separate:

```text
WebSocket or HTTP polling
  -> Engine.IO message
    -> Socket.IO event or acknowledgement, namespace, and optional ID
      -> application event name and positional arguments
        -> nested object fields
```

A text frame beginning with `42` can represent an Engine.IO message containing a
Socket.IO event. Parse the framing and namespace/ID before treating the
remaining JSON as application data. Binary attachments and polling framing
require their respective protocol handling. Payload unit tests should target the
layer actually accepted by the locked sioc API.

## Read-only runtime escalation

Use the browser skill and approved existing Edge connection when static source
leaves a specific question. Inspect loaded script/resource URLs and existing
network traffic first. Verify a global socket binding from source before reading
it. Read selected properties such as connection state, namespace, and manager
endpoint while excluding authentication data and personal values.

If the socket exists only in a closure, use available debugger facilities when
supported or follow the module graph statically. An absent `window.io` does not
mean Socket.IO is absent. Avoid executing a downloaded bundle in Node to obtain
exports because application initialization can connect or change remote state.
Use passive observations rather than emitting probe packets. Hooks that alter
methods, listeners, or timing require an explicit explanation and approval.

## Worked example from JS1

The source identity is recorded in `ground-truth.md`. The following offsets are
zero-based JavaScript UTF-16 code units in that exact bundle. Module numbers and
minified aliases are build-specific and must be rediscovered after an update.

- Around offset 2266650, module `8007` exports `io`, `connect`, `Socket`, and
  `Manager`. Its lookup function creates or reuses a manager and returns
  `c.socket(n.path,e)`. The debug string is `socket.io-client`.
- In the application region starting around 75500, `var u=n(8007).io` imports
  that export. Around 79116, `u(Z[it],{secure:...})` constructs candidates
  stored in `rt`. The preceding hostname switch selects endpoint candidates.
- At 79475, the single-candidate path assigns `At=rt[0],n.g.socket=At`. At
  80690, the measured-selection path assigns `At=rt[A.socketIndex]` and
  disconnects unselected candidates. Those assignments establish `At` as the
  application socket in this scope.
- Around 81000, `At.emit("get_username",I,function(t){...})` passes an identity
  and an acknowledgement callback. Follow the callback's consumers to infer
  username semantics rather than treating the function as a payload slot.
- Around 2274550, the bundled socket's `emit(t,...e)` prepends the event name,
  constructs an EVENT packet, pops a final function, registers the callback, and
  assigns the acknowledgement ID. This directly supports the distinction above.
- At 92994, `At.emit("join_1v1",I,w.NBK,t,null,e)` establishes five outgoing
  argument positions, including an explicit null. Resolve each expression and
  all alternate call sites before assigning final Rust types.

A classifier run on abridged, manually verified excerpts classified the code as
application Socket.IO use and the final function as an acknowledgement callback.
That result supports triage only. The source trace above supplies the evidence.
