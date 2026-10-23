//! Client-to-server event definitions.
//!
//! Layout mirrors the positional wire format of the JS protocol.

use crate::prelude::*;
use serde::{Deserialize, Serialize};
use sioc::prelude::*;

/// Contains custom-lobby option keys, with absent values omitted when sending.
///
/// Explicit nulls and extension keys can be sent through `extra`. For a declared
/// key, the corresponding typed field stays `None` to emit that key only once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomGameOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_speed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_players: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mountain_density: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city_density: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swamp_density: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lookout_density: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observatory_density: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desert_density: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city_fairness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spawn_fairness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Contains numeric indexes from the browser's modifier catalog.
    pub modifiers: Option<Vec<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tunnel_density: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tunnel_limit_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tunnel_limit_max: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stronghold_density: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stronghold_strength_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stronghold_strength_max: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defeat_spectate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spectate_chat: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public: Option<bool>,
    #[serde(
        rename = "chatRecordingDisabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub chat_recording_disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modifier_options: Option<serde_json::Value>,
    #[serde(rename = "eventId", skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    /// Retains extension keys and explicit null resets for otherwise absent fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Retrieves the current username associated with the user ID.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "GetUsernameAck"))]
pub struct GetUsername {
    pub user_id: String,
}

/// Sets a new username for the player.
#[derive(EventType, SerializePayload)]
pub struct SetUsername {
    pub user_id: String,
    pub username: String,
    pub force_flag: ForceFlag,
    /// Preserves the explicit trailing null emitted by the browser client.
    pub reserved: (),
}

/// Checks whether the player is a supporter.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "IsSupporterAck"))]
pub struct IsSupporter {
    pub user_id: String,
}

/// Checks the moderation status for the user.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "CheckModerationAck"))]
pub struct CheckModeration {
    pub user_id: String,
}

/// Joins the FFA queue.
#[derive(EventType, SerializePayload)]
pub struct Play {
    pub user_id: String,
    pub force_flag: ForceFlag,
    /// Selects a preferred supporter color, or the default when null.
    pub color: Option<i32>,
    /// Preserves the explicit null slot emitted by the browser client.
    pub reserved: (),
}

/// Joins the 1v1 queue.
#[derive(EventType, SerializePayload)]
#[sioc(event(name = "join_1v1"))]
pub struct Join1v1 {
    pub user_id: String,
    pub force_flag: ForceFlag,
    /// Selects a preferred supporter color, or the default when null.
    pub color: Option<i32>,
    /// Reserved null slot in the wire format.
    pub reserved: (),
    /// Allows bots in the queue. The browser defaults this to true when its
    /// `queue_1v1_allow_bots` setting is absent, or when it equals `"enable"`.
    pub allow_bots: bool,
}

/// Joins the 2v2 team queue.
#[derive(EventType, SerializePayload)]
pub struct JoinTeam {
    pub team_id: Option<String>,
    pub user_id: String,
    pub force_flag: ForceFlag,
    /// Selects a preferred supporter color, or the default when null.
    pub color: Option<i32>,
    /// Preserves the explicit null slot emitted by the browser client.
    pub reserved: (),
}

/// Leaves the team identified by the current team link.
#[derive(EventType, SerializePayload)]
pub struct LeaveTeam {
    pub team_id: Option<String>,
}

/// Joins the big-team queue.
#[derive(EventType, SerializePayload)]
pub struct PlayBigTeam {
    pub user_id: String,
    pub force_flag: ForceFlag,
    /// Selects a preferred supporter color, or the default when null.
    pub color: Option<i32>,
    /// Preserves the explicit null slot emitted by the browser client.
    pub reserved: (),
}

/// Joins the big-team queue using a team-link identifier.
#[derive(EventType, SerializePayload)]
pub struct JoinBigTeam {
    pub team_id: Option<String>,
    pub user_id: String,
    pub force_flag: ForceFlag,
    /// Selects a preferred supporter color, or the default when null.
    pub color: Option<i32>,
    /// Preserves the explicit null slot emitted by the browser client.
    pub reserved: (),
}

/// Joins a private custom game.
#[derive(EventType, SerializePayload)]
pub struct JoinPrivate {
    pub queue_id: String,
    pub user_id: String,
    pub force_flag: ForceFlag,
    /// Preserves the explicit trailing null emitted by the browser client.
    pub reserved: (),
}

/// Cancels the current queue search.
#[derive(EventType, SerializePayload)]
pub struct Cancel;

/// Toggles force-start for the current queue slot.
#[derive(EventType, SerializePayload)]
pub struct SetForceStart {
    pub queue_id: String,
    pub enabled: bool,
}

/// Requests the queue-count and highest-player-rating array.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "QueueCountAck"))]
pub struct QueueCount;

/// Leaves the current game.
#[derive(EventType, SerializePayload)]
pub struct LeaveGame;

/// Queues an attack/move from one tile to an adjacent tile.
#[derive(EventType, SerializePayload)]
pub struct Attack {
    pub from_index: i32,
    pub to_index: i32,
    pub is_50_percent: bool,
    /// Identifies the local queued move for reconciliation when a game update
    /// supplies an `attackIndex`.
    pub attack_index: i32,
}

/// Pings a tile to highlight it for teammates (client emit).
#[derive(EventType, SerializePayload)]
pub struct PingTile {
    pub tile_index: i32,
}

/// Surrenders the current game.
#[derive(EventType, SerializePayload)]
pub struct Surrender;

/// Undoes the last queued move.
#[derive(EventType, SerializePayload)]
pub struct UndoMove;

/// Clears all queued moves.
#[derive(EventType, SerializePayload)]
pub struct ClearMoves;

/// Reports the beginning of holding the clear-moves control.
#[derive(EventType, SerializePayload)]
pub struct StartHoldingClearMoves;

/// Reports the end of holding the clear-moves control.
#[derive(EventType, SerializePayload)]
pub struct StopHoldingClearMoves;

/// Sets the player's rematch preference.
#[derive(EventType, SerializePayload)]
pub struct Rematch {
    pub wants_rematch: bool,
}

/// Sends a chat message to a room.
#[derive(EventType, SerializePayload)]
#[sioc(event(name = "chat_message"))]
pub struct EmitChatMessage {
    pub chat_room: String,
    pub text: String,
    pub prefix: Option<String>,
}

/// Patches custom-game options and retains the callback arguments.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "CustomActionAck"))]
pub struct SetCustomOptions {
    pub game_id: String,
    pub options: CustomGameOptions,
}

/// Makes a custom game publicly visible.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "CustomActionAck"))]
pub struct MakeCustomPublic {
    pub game_id: String,
}

/// Makes a custom game private.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "CustomActionAck"))]
pub struct MakeCustomPrivate {
    pub game_id: String,
}

/// Updates the disabled state of custom-game chat recording.
///
/// The inspected browser puts a callback before the flag, so Socket.IO serializes
/// that non-final function as null rather than registering an acknowledgement.
#[derive(EventType, SerializePayload)]
pub struct UpdateCustomChatRecording {
    pub game_id: String,
    pub reserved: (),
    pub disabled: bool,
}

/// Transfers custom-lobby host privileges to the specified lobby index.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "CustomActionAck"))]
pub struct SetCustomHost {
    pub game_id: String,
    /// Identifies the target's position in the lobby arrays.
    pub lobby_index: i32,
}

/// Kicks a player from a custom game lobby.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "CustomActionAck"))]
pub struct KickFromCustom {
    pub game_id: String,
    /// Identifies the target's position in the lobby arrays.
    pub lobby_index: i32,
}

/// Reverses a kick from a custom game lobby.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "CustomActionAck"))]
pub struct UnkickFromCustom {
    pub game_id: String,
    pub username: String,
}

/// Sets the team number for the player in a custom game.
#[derive(EventType, SerializePayload)]
pub struct SetCustomTeam {
    pub game_id: String,
    pub team: i32,
}

/// Sets the caller's preferred color in a custom lobby.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "CustomActionAck"))]
pub struct SetColor {
    pub game_id: String,
    pub color: i32,
}

/// Joins an existing party or creates a new one.
#[derive(EventType, SerializePayload)]
pub struct JoinParty {
    pub party_id: Option<String>,
    pub user_id: String,
}

/// Leaves the current party.
#[derive(EventType, SerializePayload)]
pub struct LeaveParty;

/// Invites another player to the current party.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "GenericSuccessAck"))]
pub struct InviteToParty {
    pub username: String,
}

/// Requests an invitation from the named friend to that friend's party.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "GenericSuccessAck"))]
pub struct RequestPartyInvite {
    pub username: String,
}

/// Responds to a party invite request from another player.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "RespondPartyInviteRequestAck"))]
pub struct RespondPartyInviteRequest {
    pub username: String,
    pub accepted: bool,
}

/// Loads friends lists and pending friend requests.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "LoadSocialDataAck"))]
pub struct LoadSocialData {
    pub user_id: String,
}

/// Sends a friend request to another player.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "SendFriendRequestAck"))]
pub struct SendFriendRequest {
    pub user_id: String,
    pub username: String,
}

/// Accepts an incoming friend request.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "AcceptFriendRequestAck"))]
pub struct AcceptFriendRequest {
    pub username: String,
}

/// Declines an incoming friend request.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "GenericSuccessAck"))]
pub struct DeclineFriendRequest {
    pub username: String,
}

/// Cancels an outgoing friend request.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "GenericSuccessAck"))]
pub struct CancelFriendRequest {
    pub username: String,
}

/// Removes a friend from the friends list.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "GenericSuccessAck"))]
pub struct RemoveFriend {
    pub username: String,
}

/// Requests the list of suggested/previous 2v2 teammates.
#[derive(EventType, SerializePayload)]
#[sioc(event(name = "get_2v2_teammates"))]
pub struct Get2v2Teammates {
    pub user_id: String,
    pub all_time: bool,
}

/// Fetches the leaderboard for a given mode.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "LeaderboardAck"))]
pub struct Leaderboard {
    pub leaderboard_id: String,
    pub user_id: String,
}

/// Fetches all pending notifications for the user.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "GetNotifsAck"))]
pub struct GetNotifs {
    pub user_id: String,
}

/// Clears a single notification by ID.
#[derive(EventType, SerializePayload)]
pub struct ClearNotif {
    pub user_id: String,
    pub notif_id: String,
}

/// Requests the current season identifier.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "GetSeasonAck"))]
pub struct GetSeason;

/// Requests a stars/rank refresh; server responds with `stars` and `rank` events.
#[derive(EventType, SerializePayload)]
pub struct StarsAndRank {
    pub user_id: String,
}

/// Links an email address to the user account for recovery.
#[derive(EventType, SerializePayload)]
pub struct LinkEmail {
    pub user_id: String,
    pub email: String,
    pub token: String,
}

/// Starts account recovery using a previously linked email.
#[derive(EventType, SerializePayload)]
pub struct RecoverAccount {
    pub email: String,
    pub token: String,
}

/// Pings the server for latency measurement.
#[derive(EventType, SerializePayload)]
pub struct PingServer;

/// Pings the game worker for round-trip latency measurement.
#[derive(EventType, SerializePayload)]
pub struct PingWorker;

/// Joins the main menu chat room.
#[derive(EventType, SerializePayload)]
pub struct JoinMainMenuChat {
    pub user_id: String,
}

/// Leaves the main menu chat room.
#[derive(EventType, SerializePayload)]
pub struct LeaveMainMenuChat;

/// Subscribes to `public_customs_update` events.
#[derive(EventType, SerializePayload)]
pub struct ListenPublicCustoms;

/// Unsubscribes from `public_customs_update` events.
#[derive(EventType, SerializePayload)]
pub struct StopListenPublicCustoms;

/// Sets the player's stay-party preference after game over.
#[derive(EventType, SerializePayload)]
pub struct StayParty {
    pub wants_stay: bool,
}

/// Reports the game-over state to the server.
#[derive(EventType, SerializePayload)]
pub struct GameOverState;

/// Reports main-menu activity to keep the session alive.
#[derive(EventType, SerializePayload)]
pub struct MainMenuActivity;

/// Removes a member from the caller's party.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "GenericSuccessAck"))]
pub struct KickFromParty {
    pub username: String,
}

/// Blocks social interactions from the specified player.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "GenericSuccessAck"))]
pub struct BlockSocial {
    pub username: String,
}

/// Removes a player from the social block list.
#[derive(EventType, SerializePayload)]
#[sioc(event(ack = "GenericSuccessAck"))]
pub struct UnblockSocial {
    pub username: String,
}

/// Identifies a chat message for a moderator removal request.
#[derive(Debug, Serialize)]
pub struct ModeratedChatMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub text: String,
    pub prefix: String,
    /// Preserves the server-supplied timestamp used to identify a message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<serde_json::Value>,
}

/// Requests removal of a matching message using moderator permissions.
#[derive(EventType, SerializePayload)]
pub struct ModerateRemoveChatMessage {
    pub chat_room: String,
    pub message: ModeratedChatMessage,
}

/// Requests redaction of a player's visible chat using moderator permissions.
#[derive(EventType, SerializePayload)]
pub struct ModerateRedactPlayerChat {
    pub chat_room: String,
    pub username: String,
}
