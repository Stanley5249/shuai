# Socket.IO extraction

## Find the application boundary

1. Search stable anchors such as `socket.io-client`, `engine.io-client`,
   `/socket.io`, `receiveBuffer`, and `acks`. Identify their enclosing modules.
2. Trace the Socket.IO `io`/`connect` export back through imports to constructor
   calls. Resolve endpoint, namespace, options, and socket-selection branches.
3. Follow assignments and wrappers to application emitters and listeners.
   Respect lexical scope because minifiers reuse short names across modules.
4. Inspect `.emit`/`.on`/`.once`, bracket access, `.apply`, bound methods,
   spreads, registration tables, and computed names. Follow imported callbacks.
5. Separate Socket.IO library events, Engine.IO packets, local simulation,
   unrelated event buses, and actual application traffic by receiver identity.

Use an available parser or complete source inspection for argument boundaries.
Commas inside nested expressions make naive argument splitting unreliable. For
AST extraction, retain each call's start/end, method, receiver, event
expression, arguments, enclosing function, and module. Inspect computed member
access and chained calls. Normalize lexical and AST records by call identity;
lexical matches may start at the member dot, while chained AST calls can share a
start offset. Verify the actual module-table boundary before counting modules.

Preserve raw bytes and hashes. JavaScript string offsets are UTF-16 code units,
not UTF-8 bytes. Formatting changes offsets, so retain original locators. For
large minified modules, generate formatted scratch views with original module
spans in their headers. Read complete relevant functions there, then recover
original locators from the raw bundle. Give reviewers these bounded views and
specific imports to trace rather than repeatedly printing a truncated bundle.
Inspect script creation, dynamic imports, module tables, and chunk loaders.
Source-map and chunk searches establish only the patterns actually checked.

## Follow payloads

`socket.emit(name, a, b, callback)` usually sends `a` and `b` while registering
the final function as an acknowledgement callback. Verify that behavior in the
bundled Socket.IO implementation. Trace its callback parameters into state and
UI to recover response objects. Timeout/promise wrappers may insert local
errors.

Trace incoming handler arguments through actions and reducers to their final
consumers. A handler accepting a parameter does not prove the server always
sends it. Truthiness and default expressions leave type/nullability questions.
Local-game producers provide useful examples, labeled separately from network
observations. Trace all call sites before claiming an exhaustive variant set.

```text
WebSocket or polling
  -> Engine.IO message
    -> Socket.IO event/ack, namespace, optional ID
      -> event name and positional arguments
        -> nested object fields
```

A `42` text prefix can represent an Engine.IO message containing a Socket.IO
event. Account for namespace, IDs, binary attachments, and polling framing. Test
the layer accepted by sioc's API rather than passing raw transport frames.

## Runtime inspection

Use loaded resource URLs and passive network observations first. A socket may
live in a module closure even when `window.io` is absent. Follow source-proven
global bindings or available debugger facilities rather than executing the
bundle outside the browser, where initialization could connect or mutate state.
Read endpoint/namespace metadata without exposing credentials. Explain and seek
approval for hooks that change methods, listeners, or timing.

## Verified JS1 trace

The ledger owns JS1's URL and hash. These zero-based UTF-16 offsets apply only
to that build; rediscover aliases and module IDs for new builds.

- Module `8007` spans `[2266763, 2267866)` and exports `io`, `connect`,
  `Socket`, and `Manager`. Its lookup returns `c.socket(n.path,e)`.
- Application module `4080` spans `[68394, 102789)`. Around 75500,
  `var u=n(8007).io` imports the factory. Around 79116, `u(Z[it],...)` creates
  candidates in `rt`. The preceding hostname switch chooses endpoints.
- At 79475 and 80690, selection assigns `At=rt[...]` and `n.g.socket=At`.
- At 81422, `At.emit("get_username",I,function(t){...})` registers an ack.
- Socket class module `6214` spans `[2273634, 2281383)`. Its `emit(t,...e)`
  prepends the event name, pops a final function, registers it, and assigns the
  packet's acknowledgement ID.
- At 92985, `At.emit("join_1v1",I,w.NBK,t,null,e)` establishes five slots,
  including explicit null. Expression meanings require producer tracing.

The ledger records the AST candidate counts, their module classification, and
the source-coverage limits. These counts describe the fixed build only.
