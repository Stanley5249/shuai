# Protocol evidence ledger

The parent `SKILL.md` defines evidence levels and the update procedure.
`socket-extraction.md` records the verified socket-boundary trace. Rust defines
the implemented API, while this ledger records upstream observations and gaps.

## Audit status

Live source retrieval, lexical extraction, application socket tracing, and a
classifier exercise completed. Full protocol shape comparison remains partial.
All counts below describe their stated search scope rather than complete server
protocol coverage. No runtime packets were observed.

## Sources

| ID   | Identity                                                                                                                                                  | Coverage                                                                                                                                            |
| ---- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| H1   | `https://generals.io/` referenced `/generals-main-prod-v31.4.4-4a0b2a75.js`                                                                               | Public HTML entry discovery. Final redirect metadata and HTML hash were not retained.                                                               |
| JS1  | `https://generals.io/generals-main-prod-v31.4.4-4a0b2a75.js`, 2,342,208 bytes, SHA-256 `6aaa8124820c313f4d4ee90661ab4c5dc047a8704752d1d6322ca18ef28a2d51` | Downloaded raw bundle. Identity and direct source excerpts checked during this run on 2026-10-22 UTC. Exact retrieval timestamp remains unrecorded. |
| SIOC | `sioc` and `sioc-macros` versions resolved by `Cargo.lock`                                                                                                | Reviewers inspected the locked 0.5.0 derives. Focused payload tests passed.                                                                         |

JS1 contains 2,309,885 JavaScript UTF-16 code units. Offsets in the worked
example and evidence table below use that coordinate system. The extraction
artifacts also record UTF-8 byte offsets. Temporary downloads and candidate JSON
live outside the repository and may expire. The earlier draft's retrieval date
was unverified and has been replaced with the observed inspection date.

## Extraction coverage

The automated lexical scan produced 194 `.emit`, `.on`, and `.once` candidates,
including application and library emitters. Counts were 121 emits, 72 listeners,
and one once-listener. Seven listener candidates had nonliteral event names.
These candidates still require receiver-aware classification.

Direct inspection traced module `8007`'s `io` export to candidate sockets `rt`
and the selected application socket `At`. A separate deterministic search of
literal `At.emit` calls found 71 sites and 66 distinct names. Literal `At.on`
calls produced 59 sites and 59 names, including two local lifecycle events. This
scope excludes other aliases and alternate call syntax.

### Outgoing names on At

```text
accept_friend_request attack block_social cancel cancel_friend_request
chat_message check_moderation clear_moves clear_notif decline_friend_request
game_over_state get_2v2_teammates get_notifs get_season get_username
invite_to_party is_supporter join_1v1 join_big_team join_main_menu_chat
join_party join_private join_team kick_from_custom kick_from_party leaderboard
leave_game leave_main_menu_chat leave_party leave_team link_email
listen_public_customs load_social_data main_menu_activity make_custom_private
make_custom_public moderate_redact_player_chat moderate_remove_chat_message
ping_server ping_tile ping_worker play play_big_team queue_count recover_account
rematch remove_friend request_party_invite respond_party_invite_request
send_friend_request set_color set_custom_host set_custom_options set_custom_team
set_force_start set_username stars_and_rank start_holding_clear_moves stay_party
stop_holding_clear_moves stop_listen_public_customs surrender unblock_social
undo_move unkick_from_custom update_custom_chat_recording
```

### Incoming names on At

```text
2v2_teammates afk_warning big_team_update chat_color_change chat_message
chat_redaction connect disable_rematch disable_stay_party disconnect
error_banned error_join_queue error_kicked error_queue_full error_user_id
friend_added friend_offline friend_online friend_presence friend_removed
friend_request_received game_lost game_over game_start game_update game_won
gio_error joined_party left_party main_menu_chat_user_left moderation_warning
notify party_chat_history party_invite party_invite_request party_join_error
party_joined_queue party_update party_user_left ping_tile pong_server pong_worker
pre_game_start public_customs_update queue_chat_history queue_left queue_update
queue_waiting queue_waiting_over rank rematch_update removed_from_queue
server_down server_restart stars stay_party_update team_joined_queue team_update
warn_popup
```

`connect` and `disconnect` are lifecycle events, excluded from application
payload comparison. Other socket aliases need examination, including the
candidate-connection listener for `connect_error`. The Rust `ErrorSetUsername`
mapping requires a separate source trace because the At-only search omits it.
Local-game simulation emits and library emitters are distinct from production
server observations, even when they use matching event names.

## Verified observations and pending changes

Paths in the Rust column are under `crates/shuai-client/src/`.

| Event or mechanism          | JS1 evidence                                                                                  | Rust mapping and assessment                                                              | Status                                                                                |
| --------------------------- | --------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| Socket selection            | Offsets 79116, 79475, 80690 trace `io` construction, `rt`, and `At`                           | Establishes the application receiver for this scope                                      | Confirmed source trace                                                                |
| Acknowledgement extraction  | Around 2274550, `emit(t,...e)` pops a final function and assigns a callback ID                | Callback functions are separate from outgoing payload fields                             | Confirmed library behavior                                                            |
| `join_1v1`                  | At 92994, `At.emit("join_1v1",I,w.NBK,t,null,e)`                                              | `client/event.rs`, `Join1v1`, has five positional fields including a reserved unit slot  | Confirmed observed order; full expression types remain contextual                     |
| `start_holding_clear_moves` | Around 97445, `startHoldingClearMoves` emits the name with zero arguments outside local games | `StartHoldingClearMoves` documents reporting control hold start                          | Implemented; exact payload test passed                                                |
| `stop_holding_clear_moves`  | Around 97547, `stopHoldingClearMoves` emits the name with zero arguments outside local games  | `StopHoldingClearMoves` documents reporting control hold end                             | Implemented; exact payload test passed                                                |
| `stay_party_update`         | At 85042, handler forwards `(t,e,n)` to `stayPartyUpdate`                                     | `server/event.rs`, `StayPartyUpdate`, captures remaining positional values using flatten | Three observed handler parameters; meanings unresolved                                |
| `check_moderation`          | Around 82526, callback uses `(t,e,n)` for muted, disabled, and warning behavior               | `server/ack.rs`, `CheckModerationAck`, has three positional fields                       | Consumer meanings supported; named Rust fields are compatible with positional derives |
| `load_social_data`          | Around 81993 and 101116, callbacks consume `blockedPlayers` with an empty-list fallback       | `LoadSocialDataAckInner` currently omits that property                                   | Confirmed consumer access; element shape and nullability unresolved                   |
| `send_friend_request`       | Around 101710, callback checks `friendAdded` and `success`                                    | `SendFriendRequestAckInner` currently omits `success`                                    | Confirmed consumer access; exact value domain inferred                                |

The additional observed outgoing names `moderate_remove_chat_message`,
`moderate_redact_player_chat`, `kick_from_party`, `block_social`, and
`unblock_social` are candidates for coverage additions. Trace their argument
producers and acknowledgement consumers before choosing types.

Observed callback-bearing requests include `get_username`, `is_supporter`,
`get_notifs`, `load_social_data`, `check_moderation`, `leaderboard`,
`send_friend_request`, `accept_friend_request`, `decline_friend_request`,
`cancel_friend_request`, `remove_friend`, `block_social`, and `unblock_social`.
The initial acknowledgement report missed the last two, so its claim of a
complete acknowledgement inventory was rejected.

## sioc mapping findings

Review of locked derive sources established that ordinary payload fields consume
one positional element each. An `Option<T>` field accepts a null element but
still requires its slot. A flattened field collects the remaining positional
elements, rather than wrapping them as one nested array argument. Non-strict
payload decoding drains extra arguments. Event names default to snake case and
explicit overrides supply literal names. Tests in
`crates/shuai-client/src/client/event_tests.rs` verify the new event names,
zero-argument payloads, `Join1v1` ordering, `CheckModerationAck` decoding,
flattened values, and missing versus null optional slots. Fixtures are
synthetic, not runtime captures.

## Classifier exercise

The user-configured guard model list was used in order. `opencode/jev-1.13-free`
successfully classified abridged, source-verified socket construction and
`get_username` emission excerpts as application code. It classified the final
function argument as an acknowledgement callback with reported confidence 0.8.
The source trace, rather than that score, establishes the claim.

An earlier attempt with an unconfigured paid model failed with HTTP 402 for
insufficient funds. The skill now directs runs to the guard configuration and
requires approval before an unconfigured paid fallback.

## Remaining coverage

- Resolve all candidate receiver identities, computed names, alias calls, and
  imported handler consumers. Read bounded artifacts completely.
- Complete chunk/import discovery. The first scanner's zero matches for a few
  webpack indicators establish only a limited heuristic result.
- Review every event's fields, acknowledgement variants, and docstrings. Current
  source-consumer evidence supports only the explicitly listed claims.
- Verify source-map availability beyond the reported missing trailing marker.
- Reconcile optional, omitted, and null cases through source logic and targeted
  runtime inspection where needed. Edge was unnecessary for the verified traces.
- Extend regression coverage as further protocol shapes are verified.

## Validation

`just fmt` and explicit skill-directory formatting completed. `just ci` passed,
including Clippy, all-target checks, 20 workspace tests (five new client tests),
doctest checks, and API documentation. Cargo emitted Windows incremental-cache
permission notes while completing successfully.

The skill was exercised from live retrieval through two source-supported event
additions, synchronized docstrings, and exact-payload tests. This narrow
end-to-end exercise passed. Full protocol synchronization remains partial as
listed under remaining coverage.
