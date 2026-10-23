//! Server-to-client event definitions.
//!
//! Keeps the incoming positional inventory together for source-to-schema audits.
//! Field domains inferred from browser consumers are recorded in the evidence
//! ledger separately from local-simulation examples and production observations.

use crate::prelude::*;
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use sioc::prelude::*;
use thiserror::Error;

// Game state

/// Contains a chess-clock snapshot using seconds for each player's remaining time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChessClockState {
    pub player_clocks: Vec<f64>,
    pub running_players: Vec<i32>,
    /// Gives the Unix timestamp in milliseconds when clock decay begins.
    pub decay_starts_at: Option<f64>,
}

/// Configures chess-clock timing for the game.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChessClockConfig {
    /// Gives the increment in seconds applied at each farm round.
    pub clock_increment: f64,
    /// Gives the initial clock in seconds.
    pub starting_clock: f64,
    pub decay_buffer_ms: f64,
    pub dont_limit_move_speed: bool,
}

/// Identifies a tile ping included in a game-update batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TilePingData {
    pub tile_index: i32,
    pub player_color: i32,
    pub player_index: Option<i32>,
}

/// Describes the game, player slots, options, and optional clock state.
///
/// Mixed casing follows the wire keys. Nullable username slots are preserved.
#[derive(Debug, Serialize, Deserialize)]
pub struct GameStartData {
    /// Identifies the local player, or `INDEX_SPECTATOR` for a spectator.
    #[serde(rename = "playerIndex")]
    pub player_index: i32,
    pub replay_id: Option<String>,
    pub chat_room: String,
    pub team_chat_room: Option<String>,
    /// Preserves supplied names and null entries without substituting labels.
    pub usernames: Vec<Option<String>>,
    #[serde(default)]
    pub teams: Option<Vec<i32>>,
    pub game_type: String,
    #[serde(default, rename = "playerColors")]
    pub player_colors: Option<Vec<i32>>,
    #[serde(default)]
    pub queue_chat: Option<Vec<ChatMessageData>>,
    #[serde(default)]
    pub swamps: Option<Vec<i64>>,
    #[serde(default)]
    pub deserts: Option<Vec<i64>>,
    #[serde(default)]
    pub lights: Option<Vec<i64>>,
    #[serde(default)]
    pub lookouts: Option<Vec<i64>>,
    #[serde(default)]
    pub observatories: Option<Vec<i64>>,
    pub options: CustomGameOptions,
    /// Identifies the team link used by the play-again control.
    pub team_2v2_play_again: Option<String>,
    pub big_team_play_again: Option<String>,
    #[serde(rename = "chessClock")]
    pub chess_clock: Option<ChessClockState>,
    #[serde(rename = "chessClockConfig")]
    pub chess_clock_config: Option<ChessClockConfig>,
}

/// Starts the client's pre-game countdown state.
#[derive(Debug, EventType, DeserializePayload)]
pub struct PreGameStart;

/// Supplies the game's initial metadata.
#[derive(Debug, EventType, DeserializePayload)]
pub struct GameStart {
    pub data: Box<GameStartData>,
}

/// Contains one player's score and elimination state.
#[derive(Debug, Serialize, Deserialize)]
pub struct Score {
    /// Counts armies, with -1 hiding the value under Silent War.
    pub total: i64,
    /// Counts tiles, with -1 hiding the value under Silent War.
    pub tiles: i64,
    pub i: i32,
    pub color: i32,
    /// Marks a player credited with a capture; defaults to false when omitted.
    #[serde(default)]
    pub has_kill: bool,
    #[serde(default)]
    pub dead: bool,
    #[serde(default)]
    /// Measures elapsed neutralization progress as a fraction, not a turn count.
    pub neutralize_in: Option<f64>,
    /// Marks an inactivity warning for this player.
    pub warn_afk: Option<bool>,
}

/// Contains map diffs, scores, and recipient-specific game observations.
///
/// Rating arrays preserve null entries while account ratings are unavailable.
/// Optional clock and ping data accompany ordinary and heartbeat updates.
#[derive(Debug, Serialize, Deserialize)]
pub struct GameUpdateData {
    /// Counts half-turns in ordinary-speed games.
    pub turn: i64,
    pub scores: Vec<Score>,
    /// Acknowledges a queued attack index when supplied for this recipient.
    #[serde(rename = "attackIndex")]
    pub attack_index: Option<i64>,
    pub generals: Vec<i64>,
    /// Encodes copy/insert runs over the previous serialized map.
    pub map_diff: Vec<i64>,
    pub cities_diff: Vec<i64>,
    #[serde(default)]
    pub deserts_diff: Option<Vec<i64>>,
    #[serde(default)]
    pub stars: Option<Vec<Option<f64>>>,
    #[serde(default)]
    pub uncertainties: Option<Vec<Option<f64>>>,
    #[serde(default)]
    pub alt_stars: Option<Vec<Option<serde_json::Value>>>,
    #[serde(default, rename = "rawVisible")]
    pub raw_visible: Option<Vec<i64>>,
    /// Carries batched pings, notably for spectators.
    pub pings: Option<Vec<TilePingData>>,
    #[serde(rename = "chessClock")]
    pub chess_clock: Option<ChessClockState>,
}

/// Supplies incremental map, score, and observation data.
#[derive(Debug, EventType, DeserializePayload)]
pub struct GameUpdate {
    pub data: GameUpdateData,
}

/// Marks the end of the game without interpreting additional arguments.
#[derive(Debug, EventType, DeserializePayload)]
pub struct GameOver;

/// Contains the loss details consumed by the game-over display.
#[derive(Debug, Deserialize)]
pub struct GameLostData {
    #[serde(default)]
    pub surrender: bool,
    /// Identifies the capturing player when the loss was a capture.
    pub killer: Option<i32>,
    #[serde(default)]
    pub afk: bool,
    #[serde(default)]
    pub no_general: bool,
}

/// Reports the local player's elimination.
#[derive(Debug, EventType, DeserializePayload)]
pub struct GameLost {
    pub data: GameLostData,
}

/// Marks the local player's victory without interpreting additional arguments.
#[derive(Debug, EventType, DeserializePayload)]
pub struct GameWon;

// Game mechanics

/// Disables the client's rematch control.
#[derive(Debug, EventType, DeserializePayload)]
pub struct DisableRematch;

/// Reports the current rematch vote count and the local player's preference.
#[derive(Debug, EventType, DeserializePayload)]
pub struct RematchUpdate {
    pub num_rematch: i32,
    pub is_rematching: bool,
}

/// Enables the client's inactivity warning.
#[derive(Debug, EventType, DeserializePayload)]
pub struct AfkWarning;

/// Shows a warning with optional ping-blocking and display durations.
#[derive(Debug, EventType, DeserializePayload)]
pub struct WarnPopup {
    pub warn_message: String,
    /// Retains positional milliseconds: ping blocking, then popup duration.
    ///
    /// The browser defaults omitted slots to 0 and 10000 respectively. Null
    /// stays distinct from omission through the retained slot count.
    #[sioc(flatten)]
    pub timing_ms: Vec<Option<f64>>,
}

/// Highlights a tile with the supplied teammate color.
#[derive(Debug, EventType, DeserializePayload)]
#[sioc(event(name = "ping_tile"))]
pub struct PingTileOn {
    pub tile_index: i32,
    pub player_color: i32,
}

// Queue and matchmaking

/// Contains the queue and lobby fields consumed by the browser.
#[derive(Debug, Clone, Deserialize)]
pub struct QueueUpdateData {
    /// Identifies the lobby represented by this update.
    pub id: Option<String>,
    #[serde(default, rename = "numPlayers")]
    pub num_players: Option<i32>,
    #[serde(default)]
    pub usernames: Option<Vec<Option<String>>>,
    #[serde(default)]
    pub teams: Option<Vec<i32>>,
    #[serde(default, rename = "playerColors")]
    pub player_colors: Option<Vec<i32>>,
    #[serde(default, rename = "playerIndices")]
    pub player_indices: Option<Vec<i32>>,
    #[serde(default, rename = "lobbyIndex")]
    pub lobby_index: Option<i32>,
    #[serde(default)]
    pub owner: Option<i32>,
    #[serde(default, rename = "numForce")]
    pub num_force: Option<Vec<i32>>,
    #[serde(default)]
    pub options: Option<CustomGameOptions>,
    #[serde(default, rename = "queueTimeLeft")]
    pub queue_time_left: Option<f64>,
    #[serde(default)]
    /// Contains the server's waiting message displayed instead of a countdown.
    pub wait: Option<String>,
    #[serde(default)]
    pub waiting: Option<bool>,
    #[serde(default, rename = "isForcing")]
    pub is_forcing: Option<bool>,
    #[serde(default, rename = "playerCap")]
    pub player_cap: Option<i32>,
    #[serde(default, rename = "teamSize")]
    pub team_size: Option<i32>,
    #[serde(default)]
    pub bans: Option<Vec<String>>,
    #[serde(default)]
    pub kicks: Option<Vec<String>>,
    pub prestart: Option<bool>,
    /// Retains lobby extensions whose wire schemas are not yet established.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Supplies a queue or lobby update object.
#[derive(Debug, EventType, DeserializePayload)]
pub struct QueueUpdate {
    pub data: Box<QueueUpdateData>,
}

/// Updates the 2v2 team queue's player count.
#[derive(Debug, EventType, DeserializePayload)]
pub struct TeamUpdate {
    pub num_players: i32,
}

/// Updates the big-team queue's player count.
#[derive(Debug, EventType, DeserializePayload)]
pub struct BigTeamUpdate {
    pub num_players: i32,
}

/// Moves the client from the team lobby into its queue state.
#[derive(Debug, EventType, DeserializePayload)]
pub struct TeamJoinedQueue;

/// Reports removal from a team queue and identifies the team for rejoining.
#[derive(Debug, EventType, DeserializePayload)]
pub struct RemovedFromQueue {
    pub team_id: String,
}

/// Clears the client's queue state.
#[derive(Debug, EventType, DeserializePayload)]
pub struct QueueLeft;

/// Sets the client's queue-waiting state.
#[derive(Debug, EventType, DeserializePayload)]
pub struct QueueWaiting;

/// Clears the client's queue-waiting state.
#[derive(Debug, EventType, DeserializePayload)]
pub struct QueueWaitingOver;

/// Selects the party's queue and waiting state.
#[derive(Debug, EventType, DeserializePayload)]
pub struct PartyJoinedQueue {
    pub queue_id: String,
    pub is_waiting: bool,
}

// Chat

/// Contains a player or system message consumed by the chat display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageData {
    #[serde(default)]
    pub username: Option<String>,
    pub text: String,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default, rename = "playerColor")]
    pub player_color: Option<i32>,
    #[serde(default, rename = "playerIndex")]
    pub player_index: Option<i32>,
    #[serde(default, rename = "isPrevGame")]
    pub is_prev_game: Option<bool>,
    #[serde(default)]
    pub turn: Option<i32>,
    #[serde(default, rename = "multiText")]
    pub multi_text: Option<Vec<String>>,
    #[serde(default, rename = "multiColors")]
    pub multi_colors: Option<Vec<Option<i32>>>,
    /// Preserves the server's message timestamp for moderation matching.
    pub timestamp: Option<serde_json::Value>,
}

/// Delivers one message to a chat room.
#[derive(Debug, EventType, DeserializePayload)]
#[sioc(event(name = "chat_message"))]
pub struct RecvChatMessage {
    pub chat_room: String,
    pub data: ChatMessageData,
}

/// Removes matching messages from the client's chat history.
#[derive(Debug, EventType, DeserializePayload)]
pub struct ChatRedaction {
    pub chat_room: String,
    pub data: ChatRedactionData,
}

/// Matches one message or all visible messages from a username for redaction.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatRedactionData {
    #[serde(rename = "redactUsername")]
    pub redact_username: Option<bool>,
    pub username: Option<String>,
    pub text: Option<String>,
    pub prefix: Option<String>,
    #[serde(rename = "playerColor")]
    pub player_color: Option<i32>,
}

/// Maps an old player color to its replacement in stored chat messages.
#[derive(Debug, Clone, Deserialize)]
pub struct ColorChange {
    #[serde(rename = "oldColor")]
    pub old_color: i32,
    #[serde(rename = "newColor")]
    pub new_color: i32,
}

/// Replaces player colors in stored messages for a chat room.
#[derive(Debug, EventType, DeserializePayload)]
pub struct ChatColorChange {
    pub chat_room: String,
    pub changes: Vec<ColorChange>,
}

/// Supplies historical messages for a queue chat room.
#[derive(Debug, EventType, DeserializePayload)]
pub struct QueueChatHistory {
    pub chat_room: String,
    pub messages: Vec<ChatMessageData>,
}

/// Supplies historical messages for a party chat room.
#[derive(Debug, EventType, DeserializePayload)]
pub struct PartyChatHistory {
    pub party_id: String,
    pub messages: Vec<ChatMessageData>,
}

/// Marks a user's stored messages as disconnected in a chat room.
#[derive(Debug, EventType, DeserializePayload)]
pub struct MainMenuChatUserLeft {
    pub chat_room: String,
    pub username: String,
}

// Connectivity

/// Completes the client's server-ping timing sample.
#[derive(Debug, EventType, DeserializePayload)]
pub struct PongServer;

/// Supplies the worker-ping value displayed by the browser in milliseconds.
#[derive(Debug, EventType, DeserializePayload)]
pub struct PongWorker {
    pub worker_ping_ms: i32,
}

// Party

/// Identifies a party member and whether that member is the host.
#[derive(Debug, Clone, Deserialize)]
pub struct PartyMember {
    pub username: String,
    #[serde(rename = "isHost")]
    pub is_host: bool,
}

/// Contains the party identifier, host username, and member roster.
#[derive(Debug, Clone, Deserialize)]
pub struct PartyData {
    pub party_id: String,
    pub members: Vec<PartyMember>,
    pub host_username: String,
}

/// Confirms the party identifier and whether the local player is its host.
#[derive(Debug, EventType, DeserializePayload)]
pub struct JoinedParty {
    pub party_id: String,
    pub is_host: bool,
}

/// Clears party membership and the identified party's chat room.
#[derive(Debug, EventType, DeserializePayload)]
pub struct LeftParty {
    pub party_id: String,
}

/// Replaces the client's party roster and host information.
#[derive(Debug, EventType, DeserializePayload)]
pub struct PartyUpdate {
    pub data: PartyData,
}

/// Marks a departing member's stored party-chat messages as disconnected.
#[derive(Debug, EventType, DeserializePayload)]
pub struct PartyUserLeft {
    pub username: String,
}

/// Identifies an invitation's party and inviter.
#[derive(Debug, Clone, Deserialize)]
pub struct PartyInviteData {
    pub party_id: String,
    pub inviter_username: String,
}

/// Adds an invitation to the client's party-invite list.
#[derive(Debug, EventType, DeserializePayload)]
pub struct PartyInvite {
    pub data: PartyInviteData,
}

/// Identifies the requesting player and their friend in the party.
#[derive(Debug, Clone, Deserialize)]
pub struct PartyInviteRequestData {
    pub requester_username: String,
    pub friend_username: String,
}

/// Adds a request to the client's party-invite-request list.
#[derive(Debug, EventType, DeserializePayload)]
pub struct PartyInviteRequest {
    pub data: PartyInviteRequestData,
}

// Friends

/// Identifies a friend, optionally carrying party presence.
#[derive(Debug, Clone, Deserialize)]
pub struct UsernamePayload {
    pub username: String,
    #[serde(default, rename = "inParty")]
    pub in_party: Option<bool>,
}

/// Moves a friend into the online list.
#[derive(Debug, EventType, DeserializePayload)]
pub struct FriendOnline {
    pub data: UsernamePayload,
}

/// Moves a friend into the offline list.
#[derive(Debug, EventType, DeserializePayload)]
pub struct FriendOffline {
    pub data: UsernamePayload,
}

/// Identifies a new friend and the online list used by the browser.
#[derive(Debug, Clone, Deserialize)]
pub struct FriendAddedData {
    pub username: String,
    pub online: bool,
    #[serde(default, rename = "inParty")]
    pub in_party: Option<bool>,
}

/// Adds a friend and removes matching pending requests.
#[derive(Debug, EventType, DeserializePayload)]
pub struct FriendAdded {
    pub data: FriendAddedData,
}

/// Removes a friend from the client's social lists.
#[derive(Debug, EventType, DeserializePayload)]
pub struct FriendRemoved {
    pub data: UsernamePayload,
}

/// Adds an incoming friend request.
#[derive(Debug, EventType, DeserializePayload)]
pub struct FriendRequestReceived {
    pub data: UsernamePayload,
}

/// Contains a friend's contextual presence update.
#[derive(Debug, Clone, Deserialize)]
pub struct FriendPresenceData {
    pub username: String,
    /// Retains the presence object, including its state and contextual fields.
    pub presence: serde_json::Value,
    #[serde(default, rename = "inParty")]
    pub in_party: Option<bool>,
}

/// Updates an online friend's contextual presence.
#[derive(Debug, EventType, DeserializePayload)]
pub struct FriendPresence {
    pub data: FriendPresenceData,
}

// Stats and leaderboards

/// Merges rating values keyed by mode into the client's stored ratings.
#[derive(Debug, EventType, DeserializePayload)]
pub struct Stars {
    pub data: serde_json::Value,
}

/// Merges rank values keyed by mode into the client's stored ranks.
#[derive(Debug, EventType, DeserializePayload)]
pub struct Rank {
    pub data: serde_json::Value,
}

/// Describes a teammate's rating and rank.
#[derive(Debug, Clone, Deserialize)]
pub struct Teammate {
    pub username: String,
    pub stars: Option<f64>,
    pub rank: Option<i64>,
}

/// Returns suggested or historical 2v2 teammate rating rows.
#[derive(Debug, EventType, DeserializePayload)]
#[sioc(event(name = "2v2_teammates"))]
pub struct Teammates2v2 {
    pub teammates: Vec<Teammate>,
    pub is_all_time: bool,
}

// Public customs

/// Contains the fields rendered in a public custom-game listing.
#[derive(Debug, Clone, Deserialize)]
pub struct PublicCustomGame {
    pub id: String,
    /// Contains the host's display name when available.
    pub host: Option<String>,
    pub map: Option<String>,
    /// Preserves the displayed player count or count label.
    pub players: Option<serde_json::Value>,
    /// Contains modification labels paired with numeric, string, or null values.
    #[serde(default)]
    pub mods: Vec<(String, serde_json::Value)>,
}

/// Adds, replaces, or removes one public custom-game listing.
#[derive(Debug, EventType, DeserializePayload)]
pub struct PublicCustomsUpdate {
    pub game: PublicCustomGame,
    pub removed: bool,
}

// Server status and notifications

/// Displays a notification title and body.
#[derive(Debug, EventType, DeserializePayload)]
pub struct Notify {
    pub title: String,
    pub body: String,
}

/// Enables the client's server-restart notice.
#[derive(Debug, EventType, DeserializePayload)]
pub struct ServerRestart;

/// Supplies the server-down message and optional reboot time.
#[derive(Debug, EventType, DeserializePayload)]
pub struct ServerDown {
    pub message: String,
    pub reboot_timestamp_ms: Option<i64>,
}

// Errors

/// Supplies the result handled by the username dialog's error listener.
#[derive(Debug, Error, Diagnostic, EventType, DeserializePayload)]
#[error("set username failed")]
#[diagnostic(code(generals_io::error::set_username))]
pub struct ErrorSetUsername {
    #[help]
    pub message: Option<String>,
}

/// Reports a conflicting connection for the user ID.
#[derive(Debug, Error, Diagnostic, EventType, DeserializePayload)]
#[error("user ID is already connected")]
#[diagnostic(code(generals_io::error::user_id))]
pub struct ErrorUserId;

/// Reports that the requested queue is full.
#[derive(Debug, Error, Diagnostic, EventType, DeserializePayload)]
#[error("queue is full")]
#[diagnostic(code(generals_io::error::queue_full))]
pub struct ErrorQueueFull;

/// Reports a queue ban, including any supplied browser-facing message.
#[derive(Debug, Error, Diagnostic, EventType, DeserializePayload)]
#[error("temporarily banned")]
#[diagnostic(code(generals_io::error::banned))]
pub struct ErrorBanned {
    #[help]
    pub message: Option<String>,
}

/// Reports a kick, including any supplied browser-facing message.
#[derive(Debug, Error, Diagnostic, EventType, DeserializePayload)]
#[error("kicked from game or queue")]
#[diagnostic(code(generals_io::error::kicked))]
pub struct ErrorKicked {
    #[help]
    pub message: Option<String>,
}

/// Supplies an error value whose recognized strings trigger client actions.
#[derive(Debug, Error, Diagnostic, EventType, DeserializePayload)]
#[error("server error")]
#[diagnostic(code(generals_io::error::gio_error))]
pub struct GioError {
    #[help]
    pub message: String,
}

/// Reports a failed queue join.
#[derive(Debug, Error, Diagnostic, EventType, DeserializePayload)]
#[error("unable to join queue")]
#[diagnostic(code(generals_io::error::join_queue))]
pub struct ErrorJoinQueue {
    #[help]
    pub message: Option<String>,
}

/// Shows the browser's fixed moderation warning without interpreting arguments.
#[derive(Debug, Error, Diagnostic, EventType, DeserializePayload)]
#[error("moderation warning")]
#[diagnostic(code(generals_io::error::moderation_warning), severity(Warning))]
pub struct ModerationWarning;

/// Supplies the message displayed after a failed party join.
#[derive(Debug, Error, Diagnostic, EventType, DeserializePayload)]
#[error("party join error")]
#[diagnostic(code(generals_io::error::party_join))]
pub struct PartyJoinError {
    #[help]
    pub message: Option<String>,
}

/// Disables the client's stay-party control.
#[derive(Debug, Error, Diagnostic, EventType, DeserializePayload)]
#[error("stay-party option is no longer available")]
#[diagnostic(code(generals_io::error::disable_stay_party))]
pub struct DisableStayParty;

/// Reports the stay-party vote count, local preference, and optional team size.
///
/// The third argument is optional in the client consumer. Trailing values remain
/// preserved until all server variants are established.
#[derive(Debug, EventType, DeserializePayload)]
pub struct StayPartyUpdate {
    pub num_stay_party: i32,
    pub is_staying: bool,
    /// Preserves the optional team-player-count slot, including null.
    #[sioc(flatten)]
    pub team_player_count: Vec<Option<i32>>,
}
