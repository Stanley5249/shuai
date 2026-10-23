# Protocol evidence ledger

## Source and evidence levels

This ledger maps the complete discovered application inventory for one fixed
browser build. Rust models browser-visible arguments and consumer assumptions,
rather than an exhaustive server schema. Source review includes unchanged
definitions. Passing tests and packet-name coverage are separate from unresolved
server variants.

- JS1 is the sole script of the homepage HTML. It has no source-map response
  header or sourceMappingURL marker, and the conventional `.js.map` URL returned
  HTTP 404. Other map names remain unknown.
- Coordinates below are zero-based UTF-16 offsets in raw JS1. Formatted scratch
  modules retain original module spans; formatting itself changes offsets.
- `Cargo.lock` owns the resolved sioc versions. The sioc derives consume named
  fields positionally, require ordinary Option slots, flatten trailing values,
  and drain unmodeled extra arguments.

Status **B** denotes directly observed browser emission, **C** a traced browser
consumer contract, **L** a local-simulation example, **I** an inferred primitive
domain or compatibility tolerance, and **U** an unresolved variant. C/I rows
have source-backed argument roles, while Rust string/bool/integer domains and
missing/null allowances may remain inferred. None is a passive server capture.

## Release identity

The audited browser release is **31.4.4**. All artifacts returned HTTP 200.

| Artifact        | Location                                                     | Evidence                                                                                                                                                                                         |
| --------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| JS1             | `https://generals.io/generals-main-prod-v31.4.4-4a0b2a75.js` | SHA-256 `6aaa8124820c313f4d4ee90661ab4c5dc047a8704752d1d6322ca18ef28a2d51`. 2,342,208 UTF-8 bytes; 2,309,885 UTF-16 code units. The filename carries release `v31.4.4` and asset tag `4a0b2a75`. |
| Bundled version | Module 7185 in JS1                                           | Exports `VERSION:"31.4.4"` at JS1 UTF-16 [1149129,1149145)                                                                                                                                       |
| Homepage HTML   | `https://generals.io/`                                       | SHA-256 `94afcebe76059cb873137381f404830df17da9627cfd4aba411161e3b74b0dc6`. Its sole script URL is JS1.                                                                                          |
| Version history | [Version history](https://generals.io/versions)              | Lists Version 31.4.4 first. Raw HTML is 142,084 UTF-8 bytes, SHA-256 `d9f7d1164983764d1e9449f9c13086ce1ebc9fa997964e7bd4ac402a6af4e580`.                                                         |

The markers identify the browser release and asset tag. Server revision, a
separate protocol version, and asset-tag commit provenance remain unresolved.

## Extraction coverage

The verified webpack module table has 433 modules. An Acorn AST walk found 230
emit/on/once/addListener candidates across 13 modules. Module 4080 has 131
selected-socket calls on At plus five candidate-socket calls on r. Selected
calls cover 66 outgoing names and 58 incoming application names; connect and
disconnect are lifecycle registrations. error_set_username uses once.

| Module                 | Calls | Classification                                                         |
| ---------------------- | ----- | ---------------------------------------------------------------------- |
| 4080                   | 136   | Browser socket construction, benchmarking, wrappers, and registrations |
| 3520, 3067             | 40    | Local game and combat simulation                                       |
| 1746, 6396             | 12    | GIF generation and its event emitter                                   |
| 1392                   | 5     | Firebase/internal client listener machinery                            |
| 3823                   | 2     | Internal event-emitter registration                                    |
| 8223, 528              | 21    | Engine.IO transports and polling                                       |
| 3776, 5942, 6214, 7285 | 14    | Socket.IO manager, socket, and emitter helpers                         |

All 194 older lexical candidates match AST method/call-end pairs. The 36
additional AST calls are outside module 4080 and add no application names.
Computed event expressions belong to internal helpers/emitters. Three explicit
emit.apply forms belong to Socket.IO module 6214. AST ImportExpression count is
zero. Searches found no webpackChunk, webpackJsonp, or sourceMappingURL markers.
These checks cover the inspected artifact and recognized call forms; server-only
packets and resources absent from this HTML/build remain U.

The five receiver-r calls belong to the candidate-socket latency benchmark. Its
lifecycle listeners select or reject a candidate connection; its application
ping traffic duplicates the selected socket's schema rather than adding events.

| Call             | UTF-16 span   | Disposition                                        |
| ---------------- | ------------- | -------------------------------------------------- |
| on connect_error | [79808,79903) | Candidate connection failure                       |
| on connect       | [79904,80365) | Candidate connection setup                         |
| on pong_server   | [79931,80200) | Benchmark response, same no-interpreted-slot event |
| emit ping_server | [80283,80304) | Delayed benchmark request, no arguments            |
| emit ping_server | [80313,80334) | Immediate benchmark request, no arguments          |

The application name inventory comes from the selected At socket. Its ping rows
also retain the duplicate benchmark call locators listed here.

## Outgoing inventory

Rust paths in this section are under `crates/shuai-client/src/client/`. The type
column lists Rust positional fields, not JSON object wrappers. Every row has a
sender method in `socket.rs`; callback-bearing events retain their
acknowledgement association. Individual producer domains are qualified below.

| Event                          | Rust type and slots                                                     | Ack                            | JS1 call starts     | Status |
| ------------------------------ | ----------------------------------------------------------------------- | ------------------------------ | ------------------- | ------ |
| `accept_friend_request`        | `AcceptFriendRequest` (`username`)                                      | `AcceptFriendRequestAck`       | 101957              | C/I    |
| `attack`                       | `Attack` (`from_index`, `to_index`, `is_50_percent`, `attack_index`)    | -                              | 95218               | C/I    |
| `block_social`                 | `BlockSocial` (`username`)                                              | `GenericSuccessAck`            | 102556              | C/I    |
| `cancel`                       | `Cancel` (no interpreted slots)                                         | -                              | 95820               | B      |
| `cancel_friend_request`        | `CancelFriendRequest` (`username`)                                      | `GenericSuccessAck`            | 102297              | C/I    |
| `chat_message`                 | `EmitChatMessage` (`chat_room`, `text`, `prefix`)                       | -                              | 83549, 96541        | C/I    |
| `check_moderation`             | `CheckModeration` (`user_id`)                                           | `CheckModerationAck`           | 82524               | C/I    |
| `clear_moves`                  | `ClearMoves` (no interpreted slots)                                     | -                              | 97351               | B      |
| `clear_notif`                  | `ClearNotif` (`user_id`, `notif_id`)                                    | -                              | 99384               | C/I    |
| `decline_friend_request`       | `DeclineFriendRequest` (`username`)                                     | `GenericSuccessAck`            | 102150              | C/I    |
| `game_over_state`              | `GameOverState` (no interpreted slots)                                  | -                              | 97038               | B      |
| `get_2v2_teammates`            | `Get2v2Teammates` (`user_id`, `all_time`)                               | -                              | 96923               | C/I    |
| `get_notifs`                   | `GetNotifs` (`user_id`)                                                 | `GetNotifsAck`                 | 81893               | C/I    |
| `get_season`                   | `GetSeason` (no interpreted slots)                                      | `GetSeasonAck`                 | 98943               | B      |
| `get_username`                 | `GetUsername` (`user_id`)                                               | `GetUsernameAck`               | 81422               | C/I    |
| `invite_to_party`              | `InviteToParty` (`username`)                                            | `GenericSuccessAck`            | 98616               | C/I    |
| `is_supporter`                 | `IsSupporter` (`user_id`)                                               | `IsSupporterAck`               | 81610               | C/I    |
| `join_1v1`                     | `Join1v1` (`user_id`, `force_flag`, `color`, `reserved`, `allow_bots`)  | -                              | 92985               | C/I    |
| `join_big_team`                | `JoinBigTeam` (`team_id`, `user_id`, `force_flag`, `color`, `reserved`) | -                              | 97948               | C/I    |
| `join_main_menu_chat`          | `JoinMainMenuChat` (`user_id`)                                          | -                              | 81250, 98143        | C/I    |
| `join_party`                   | `JoinParty` (`party_id`, `user_id`)                                     | -                              | 81864, 98455        | C/I    |
| `join_private`                 | `JoinPrivate` (`queue_id`, `user_id`, `force_flag`, `reserved`)         | -                              | 93119               | C/I    |
| `join_team`                    | `JoinTeam` (`team_id`, `user_id`, `force_flag`, `color`, `reserved`)    | -                              | 93050, 97883        | C/I    |
| `kick_from_custom`             | `KickFromCustom` (`game_id`, `lobby_index`)                             | `CustomActionAck`              | 94946               | C/I    |
| `kick_from_party`              | `KickFromParty` (`username`)                                            | `GenericSuccessAck`            | 98556               | C/I    |
| `leaderboard`                  | `Leaderboard` (`leaderboard_id`, `user_id`)                             | `LeaderboardAck`               | 96737               | C/I    |
| `leave_game`                   | `LeaveGame` (no interpreted slots)                                      | -                              | 93557               | B      |
| `leave_main_menu_chat`         | `LeaveMainMenuChat` (no interpreted slots)                              | -                              | 98328               | B      |
| `leave_party`                  | `LeaveParty` (no interpreted slots)                                     | -                              | 98504               | B      |
| `leave_team`                   | `LeaveTeam` (`team_id`)                                                 | -                              | 98079               | C/I    |
| `link_email`                   | `LinkEmail` (`user_id`, `email`, `token`)                               | -                              | 98831               | C/I    |
| `listen_public_customs`        | `ListenPublicCustoms` (no interpreted slots)                            | -                              | 99171               | B      |
| `load_social_data`             | `LoadSocialData` (`user_id`)                                            | `LoadSocialDataAck`            | 81991, 101114       | C/I    |
| `main_menu_activity`           | `MainMenuActivity` (no interpreted slots)                               | -                              | 98402               | B      |
| `make_custom_private`          | `MakeCustomPrivate` (`game_id`)                                         | `CustomActionAck`              | 94726               | C/I    |
| `make_custom_public`           | `MakeCustomPublic` (`game_id`)                                          | `CustomActionAck`              | 94659               | C/I    |
| `moderate_redact_player_chat`  | `ModerateRedactPlayerChat` (`chat_room`, `username`)                    | -                              | 96212               | C/I    |
| `moderate_remove_chat_message` | `ModerateRemoveChatMessage` (`chat_room`, `message`)                    | -                              | 96056               | C/I    |
| `ping_server`                  | `PingServer` (no interpreted slots)                                     | -                              | 80283, 80313, 93341 | B      |
| `ping_tile`                    | `PingTile` (`tile_index`)                                               | -                              | 95613               | C/I    |
| `ping_worker`                  | `PingWorker` (no interpreted slots)                                     | -                              | 93254               | B      |
| `play`                         | `Play` (`user_id`, `force_flag`, `color`, `reserved`)                   | -                              | 92793               | C/I    |
| `play_big_team`                | `PlayBigTeam` (`user_id`, `force_flag`, `color`, `reserved`)            | -                              | 92852               | C/I    |
| `queue_count`                  | `QueueCount` (no interpreted slots)                                     | `QueueCountAck`                | 99490               | B      |
| `recover_account`              | `RecoverAccount` (`email`, `token`)                                     | -                              | 98889               | C/I    |
| `rematch`                      | `Rematch` (`wants_rematch`)                                             | -                              | 95946               | C/I    |
| `remove_friend`                | `RemoveFriend` (`username`)                                             | `GenericSuccessAck`            | 102437              | C/I    |
| `request_party_invite`         | `RequestPartyInvite` (`username`)                                       | `GenericSuccessAck`            | 98681               | C/I    |
| `respond_party_invite_request` | `RespondPartyInviteRequest` (`username`, `accepted`)                    | `RespondPartyInviteRequestAck` | 98760               | C/I    |
| `send_friend_request`          | `SendFriendRequest` (`user_id`, `username`)                             | `SendFriendRequestAck`         | 101708              | C/I    |
| `set_color`                    | `SetColor` (`game_id`, `color`)                                         | `CustomActionAck`              | 95076               | C/I    |
| `set_custom_host`              | `SetCustomHost` (`game_id`, `lobby_index`)                              | `CustomActionAck`              | 94881               | C/I    |
| `set_custom_options`           | `SetCustomOptions` (`game_id`, `options`)                               | `CustomActionAck`              | 93187               | C/I    |
| `set_custom_team`              | `SetCustomTeam` (`game_id`, `team`)                                     | -                              | 94576               | C/I    |
| `set_force_start`              | `SetForceStart` (`queue_id`, `enabled`)                                 | -                              | 95865               | C/I    |
| `set_username`                 | `SetUsername` (`user_id`, `username`, `force_flag`, `reserved`)         | -                              | 94348               | C/I    |
| `stars_and_rank`               | `StarsAndRank` (`user_id`)                                              | -                              | 81035               | C/I    |
| `start_holding_clear_moves`    | `StartHoldingClearMoves` (no interpreted slots)                         | -                              | 97443               | B      |
| `stay_party`                   | `StayParty` (`wants_stay`)                                              | -                              | 95993               | C/I    |
| `stop_holding_clear_moves`     | `StopHoldingClearMoves` (no interpreted slots)                          | -                              | 97545               | B      |
| `stop_listen_public_customs`   | `StopListenPublicCustoms` (no interpreted slots)                        | -                              | 99277               | B      |
| `surrender`                    | `Surrender` (no interpreted slots)                                      | -                              | 97111               | B      |
| `unblock_social`               | `UnblockSocial` (`username`)                                            | `GenericSuccessAck`            | 102684              | C/I    |
| `undo_move`                    | `UndoMove` (no interpreted slots)                                       | -                              | 97229               | B      |
| `unkick_from_custom`           | `UnkickFromCustom` (`game_id`, `username`)                              | `CustomActionAck`              | 95014               | C/I    |
| `update_custom_chat_recording` | `UpdateCustomChatRecording` (`game_id`, `reserved`, `disabled`)         | -                              | 94804               | C/I    |

## Incoming inventory

Rust types are in `crates/shuai-client/src/server/event.rs` and routed through
`server/router.rs`. A `data` field represents one direct object argument. "No
interpreted slots" means the client ignores arguments; sioc still accepts
additional slots. It is not a claim that every server packet has zero arguments.

| Event                      | Rust type and slots                                                          | Ack | JS1 call starts | Status |
| -------------------------- | ---------------------------------------------------------------------------- | --- | --------------- | ------ |
| `2v2_teammates`            | `Teammates2v2` (`teammates`, `is_all_time`)                                  | -   | 90771           | C/I    |
| `afk_warning`              | `AfkWarning` (no interpreted slots)                                          | -   | 90141           | C/I    |
| `big_team_update`          | `BigTeamUpdate` (`num_players`)                                              | -   | 85821           | C/I    |
| `chat_color_change`        | `ChatColorChange` (`chat_room`, `changes`)                                   | -   | 87397           | C/I    |
| `chat_message`             | `RecvChatMessage` (`chat_room`, `data`)                                      | -   | 86295           | C/I    |
| `chat_redaction`           | `ChatRedaction` (`chat_room`, `data`)                                        | -   | 87323           | C/I    |
| `disable_rematch`          | `DisableRematch` (no interpreted slots)                                      | -   | 84809           | C/I    |
| `disable_stay_party`       | `DisableStayParty` (no interpreted slots)                                    | -   | 84885           | C/I    |
| `error_banned`             | `ErrorBanned` (`message`)                                                    | -   | 91939           | C/I    |
| `error_join_queue`         | `ErrorJoinQueue` (`message`)                                                 | -   | 91646           | C/I    |
| `error_kicked`             | `ErrorKicked` (`message`)                                                    | -   | 92347           | C/I    |
| `error_queue_full`         | `ErrorQueueFull` (no interpreted slots)                                      | -   | 91547           | C/I    |
| `error_set_username`       | `ErrorSetUsername` (`message`)                                               | -   | 94387           | C/I    |
| `error_user_id`            | `ErrorUserId` (no interpreted slots)                                         | -   | 91284           | C/I    |
| `friend_added`             | `FriendAdded` (`data`)                                                       | -   | 89222           | C/I    |
| `friend_offline`           | `FriendOffline` (`data`)                                                     | -   | 89143           | C/I    |
| `friend_online`            | `FriendOnline` (`data`)                                                      | -   | 89056           | C/I    |
| `friend_presence`          | `FriendPresence` (`data`)                                                    | -   | 89491           | C/I    |
| `friend_removed`           | `FriendRemoved` (`data`)                                                     | -   | 89316           | C/I    |
| `friend_request_received`  | `FriendRequestReceived` (`data`)                                             | -   | 89395           | C/I    |
| `game_lost`                | `GameLost` (`data`)                                                          | -   | 90081           | C/I    |
| `game_over`                | `GameOver` (no interpreted slots)                                            | -   | 84756           | C/I    |
| `game_start`               | `GameStart` (`data`)                                                         | -   | 83197           | C/I    |
| `game_update`              | `GameUpdate` (`data`)                                                        | -   | 84623           | C/I    |
| `game_won`                 | `GameWon` (no interpreted slots)                                             | -   | 90207           | C/I    |
| `gio_error`                | `GioError` (`message`)                                                       | -   | 92580           | C/I    |
| `joined_party`             | `JoinedParty` (`party_id`, `is_host`)                                        | -   | 87954           | C/I    |
| `left_party`               | `LeftParty` (`party_id`)                                                     | -   | 88059           | C/I    |
| `main_menu_chat_user_left` | `MainMenuChatUserLeft` (`chat_room`, `username`)                             | -   | 87863           | C/I    |
| `moderation_warning`       | `ModerationWarning` (no interpreted slots)                                   | -   | 91744           | C/I    |
| `notify`                   | `Notify` (`title`, `body`)                                                   | -   | 82964           | C/I    |
| `party_chat_history`       | `PartyChatHistory` (`party_id`, `messages`)                                  | -   | 88377           | C/I    |
| `party_invite`             | `PartyInvite` (`data`)                                                       | -   | 89608           | C/I    |
| `party_invite_request`     | `PartyInviteRequest` (`data`)                                                | -   | 89866           | C/I    |
| `party_join_error`         | `PartyJoinError` (`message`)                                                 | -   | 89753           | C/I    |
| `party_joined_queue`       | `PartyJoinedQueue` (`queue_id`, `is_waiting`)                                | -   | 88762           | C/I    |
| `party_update`             | `PartyUpdate` (`data`)                                                       | -   | 88181           | C/I    |
| `party_user_left`          | `PartyUserLeft` (`username`)                                                 | -   | 88590           | C/I    |
| `ping_tile`                | `PingTileOn` (`tile_index`, `player_color`)                                  | -   | 85123           | C/I    |
| `pong_server`              | `PongServer` (no interpreted slots)                                          | -   | 79931, 85187    | C/I    |
| `pong_worker`              | `PongWorker` (`worker_ping_ms`)                                              | -   | 85349           | C/I    |
| `pre_game_start`           | `PreGameStart` (no interpreted slots)                                        | -   | 82989           | C/I    |
| `public_customs_update`    | `PublicCustomsUpdate` (`game`, `removed`)                                    | -   | 99204           | C/I    |
| `queue_chat_history`       | `QueueChatHistory` (`chat_room`, `messages`)                                 | -   | 87480           | C/I    |
| `queue_left`               | `QueueLeft` (no interpreted slots)                                           | -   | 86063           | C/I    |
| `queue_update`             | `QueueUpdate` (`data`)                                                       | -   | 85588           | C/I    |
| `queue_waiting`            | `QueueWaiting` (no interpreted slots)                                        | -   | 88919           | C/I    |
| `queue_waiting_over`       | `QueueWaitingOver` (no interpreted slots)                                    | -   | 88985           | C/I    |
| `rank`                     | `Rank` (`data`)                                                              | -   | 91093           | C/I    |
| `rematch_update`           | `RematchUpdate` (`num_rematch`, `is_rematching`)                             | -   | 84966           | C/I    |
| `removed_from_queue`       | `RemovedFromQueue` (`team_id`)                                               | -   | 85989           | C/I    |
| `server_down`              | `ServerDown` (`message`, `reboot_timestamp_ms`)                              | -   | 90265           | C/I    |
| `server_restart`           | `ServerRestart` (no interpreted slots)                                       | -   | 92506           | C/I    |
| `stars`                    | `Stars` (`data`)                                                             | -   | 90899           | C/I    |
| `stay_party_update`        | `StayPartyUpdate` (`num_stay_party`, `is_staying`, `team_player_count` tail) | -   | 85040           | C/I    |
| `team_joined_queue`        | `TeamJoinedQueue` (no interpreted slots)                                     | -   | 85905           | C/I    |
| `team_update`              | `TeamUpdate` (`num_players`)                                                 | -   | 85744           | C/I    |
| `warn_popup`               | `WarnPopup` (`warn_message`, `timing_ms` tail)                               | -   | 92436           | C/I    |

## Nested shapes and source traces

| Scope                     | Source trace                                                                                                                       | Assessment                                                                                                                                                                                                                                               |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Identity and force        | Module 4080 setup and setUsername; module 7185 NBK at 1150896; username dialog module 7149                                         | B/C. Exact token uses capital O: `sdO9fjdZO3iOejwi_changeme`. Username/supporter/moderation primitive domains are I.                                                                                                                                     |
| Queue joins               | Module 4080 dt/vt/pt/mt/yt and team-link wrappers; preference helper and queue UI callers                                          | B for slot order/null placeholders and allow-bots boolean. Preferred colors are numeric selections; team-link String/null domains remain I.                                                                                                              |
| Queue object              | Module 9955 ACTION_UPDATE; queue UI modules 6207 and its imported controls                                                         | C/I. Shallow updates use camelCase numPlayers/playerColors/playerIndices/lobbyIndex/numForce/queueTimeLeft. wait/waiting defaults are reducer behavior. Unknown keys remain in extra; num_spectators has no JS1 evidence and is untyped.                 |
| Custom options            | Module 5523 constructor/includeOptionsPropertiesFrom/getPatch; custom queue UI modifier and numeric-input writers                  | B/C/I. modifiers are numeric catalog indexes. Stronghold strength controls and query import use parseInt, represented as inferred i32 values. Typed None omits a key; extra can send explicit null when its typed field is None.                         |
| Option extensions         | Module 5523 pass-through keys and getPatch; modifier UI writers                                                                    | C/U. priority_cycle, priority_reversed, old_priority, city_regen, chat_present, custom_map_data and future keys remain in extra. modifier_options preserves nested ChessClock and future values dynamically.                                             |
| Custom administration     | Module 4080 custom methods; custom queue UI callbacks                                                                              | B for arguments, final callbacks, and lobby indexes. Callback results are only truthiness-tested, so CustomActionAck preserves all values dynamically.                                                                                                   |
| Chat recording            | Module 4080 updateCustomChatRecording; custom queue enable/disable writers                                                         | B. A non-final callback serializes as null before the disabled flag; it is not a registered acknowledgement.                                                                                                                                             |
| Party roster              | Module 4080 party listeners; modules 5031, 7144, and party roster UI                                                               | C/I. joined_party has ID and host slots; party_update consumes party_id, host_username, members with username/isHost.                                                                                                                                    |
| Party invites             | Module 9944 invite callback; module 1184 toast callbacks/propTypes; module 5730 request-invite UI; module 7144 reducer             | C/I. invite_to_party reads success/reason/party_id. respond_party_invite_request ignores callback values, so its acknowledgement preserves them dynamically.                                                                                             |
| Social lists              | Module 4080 loadSocialData callbacks; module 9035 reducer and friends UI                                                           | C/I. Nullable/omittable lists fall back to an empty array. List entries need no online key; presence is a contextual object. Request and blocked entries consume username.                                                                               |
| Social actions            | Module 4080 social callbacks; module 9944 wrapper and social UI                                                                    | C/I. success/reason/friendAdded/friend properties are consumed; conventional bool/string domains are I. Complete error/result variants remain U.                                                                                                         |
| Presence                  | Module 9035 A/a helpers and ACTION_FRIEND_PRESENCE; presence UI                                                                    | C. presence is object-shaped with state and contextual keys, retained dynamically. inParty has a boolean guard in the reducer.                                                                                                                           |
| Chat messages             | Module 4080 chat handler/sender/buffering; module 9393 reducer; module 3520 local system/winner messages                           | C/L/I. Messages include nullable player metadata, multiText/multiColors segments, and optional timestamp. The moderator request copies username/text/prefix/timestamp; the timestamp domain remains U.                                                   |
| Chat redaction and colors | Module 9393 ACTION_REDACT_MESSAGE/ACTION_UPDATE_COLORS/ACTION_MARK_USER_DISCONNECTED; module 450 actions                           | C/I. Redaction matches text/prefix and username or playerColor; redactUsername removes all matching names. Color changes are arrays of oldColor/newColor objects. Timestamp is not used by this client reducer.                                          |
| Leaderboard               | Module 4080 getLeaderboard; module 7144 ACTION_LEADERBOARD; leaderboard UI                                                         | C/I. The response's ladder is required by the reducer, not injected from the request. users/supporters/f use user-slot indexes, stars/qualifiers ranked-row indexes; 2v2 uses paired user slots. f remains dynamic.                                      |
| Ratings and teammates     | Module 7144 ACTION_STARS/ACTION_RANK; teammate UI and module 4080 listener                                                         | C/I/U. Ratings/ranks are mode-keyed objects retained dynamically. Teammate rows consume username/stars/rank; all-time is a separate positional flag.                                                                                                     |
| Notifications and season  | Module 7144 notification reducer, notification UI, and module 4080 getSeason                                                       | C/U. Notifications are keyed values; season is stored directly, so its schema remains dynamic.                                                                                                                                                           |
| Public customs            | Module 7128 onUpdate/renderGame and HTTP mount callback; module 4080 public-custom methods                                         | C/I. Socket updates carry a game object plus removal flag; removal uses game.id. UI consumes host/map/players/mods tuples. HTTP bootstrap is separate evidence.                                                                                          |
| Game controls             | Module 4080 attack/ping/Ct/queued-control wrappers; module 4210 actions; module 5809 reducers                                      | B/C/I. Attack carries a local reconciliation index. ping_tile carries tile and color. warn_popup timing slots default in the action creator and are preserved as a trailing millisecond sequence.                                                        |
| Game start                | Module 4080 game_start handler; module 4210 start; module 5809 ACTION_START; module 3520 start producers                           | C/L/I. playerIndex=-1 identifies spectators. Names/nulls and optional special-tile arrays are permissive representations. Local spectators omit some player metadata; universal server variants remain U.                                                |
| Scores and updates        | Module 3520 recalculateScores/pushCurrentStateAndHandleGameOver/sendGameUpdate/sendSpectatorGameUpdates; module 5809 ACTION_UPDATE | C/L/I. leaderboardScores is sent as scores, including Silent War -1 values and omitted has_kill. neutralize_in is rounded elapsed/interval progress; capturedPlayers initialization supports boolean has_kill.                                           |
| Update variants           | Module 3520 player/spectator sending and sendChessClockHeartbeatUpdate                                                             | L/I. Heartbeats reuse scores/turn/clock plus map/city/desert diffs. Ratings have per-player null slots; alt_stars stays dynamic. attackIndex is recipient-related; omission tolerance remains I.                                                         |
| Clock and ping batches    | Module 3520 clock getters/decay functions/handlePingTile; module 4210/5809 bulk-ping consumers                                     | C/L/I. Remaining time and farm-round increments are seconds, decayStartsAt epoch milliseconds, decayBufferMs milliseconds. Spectator ping objects carry tileIndex/playerColor/playerIndex; optional playerIndex also tolerates browser-consumed absence. |
| End/rematch               | Modules 4210 and 5809 win/lose/rematch/stay-party actions/reducers; module 7760 UI; module 3520 local loss producers               | C/L/I. Loss flags and killer reach gameOverData. Vote counts/preferences are separate slots; finite teamPlayerCount updates the stay-party denominator.                                                                                                  |
| Connectivity and errors   | Module 4080 ping/error/status handlers and UI consumers                                                                            | B/C/I/U. Server ping uses Date.now; worker milliseconds follow its UI label. Server-down reboot time is compared with Date.now. Ignored notifications consume no required slot; optional error-message slot variants remain U.                           |

## Remaining uncertainty

Rejected findings include treating a named positional field as a JSON wrapper,
claiming unit payloads reject extra arguments, removing leaderboard.ladder
without tracing its reducer, and replacing conventional primitive inferences
solely because JavaScript permits coercion. The chat reducer's disconnected flag
is browser-local display state, set by ACTION_MARK_USER_DISCONNECTED after
main_menu_chat_user_left. Its assignment provides no wire-producer evidence, so
ChatMessageData stays a network message model rather than a browser-state model.

- Primitive domains and integer ranges inferred from browser controls/propTypes
  remain I rather than universal server constraints.
- Explicit null versus omission is established only where a producer constructs
  it. Reader fallbacks describe browser tolerance, not actual network packets.
- GetSeasonAck, notification values, opaque action acknowledgements,
  modifier_options, alternate ratings, presence extensions and moderator
  leaderboard markers stay narrowly dynamic.
- Party/social acknowledgement error variants, ignored callback results, some
  optional message slots, server-only events and alternate builds remain U.
- Local-game examples establish concrete implementations without standing in for
  production server observations. No passive runtime capture was used.
- Clock finiteness and valid running-player IDs are Rust state requirements.
  Local-simulation clamping describes its producer; signed finite clock values
  remain representable while universal server ranges remain U.
- Synthetic fixtures test the Rust representation and sioc behavior. They are
  separate from source evidence and packet captures.
