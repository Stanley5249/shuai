//! Server acknowledgement arguments and their nested response objects.
//!
//! Named acknowledgement fields are positional Socket.IO arguments. Nested
//! response types describe object keys within one argument. Primitive domains
//! inferred from browser usage remain distinct from observed server packets in
//! the protocol evidence ledger.

use serde::Deserialize;
use serde_json::{Map, Value};
use sioc::prelude::*;

/// Returns the username value consumed by the browser, retaining null.
#[derive(Debug, AckType, DeserializePayload)]
pub struct GetUsernameAck {
    pub username: Option<String>,
}

/// Returns the account's supporter status.
#[derive(Debug, AckType, DeserializePayload)]
pub struct IsSupporterAck {
    pub is_supporter: bool,
}

/// Returns muted status, disabled status, and a moderation warning in order.
#[derive(Debug, AckType, DeserializePayload)]
pub struct CheckModerationAck {
    pub is_muted: bool,
    pub is_disabled: bool,
    pub warning: Option<String>,
}

/// Retains custom-lobby callback arguments whose value schemas are unresolved.
///
/// The browser displays truthy first results as errors. It provides no stronger
/// result schema, so empty, null, and additional arguments remain representable.
#[derive(Debug, AckType, DeserializePayload)]
pub struct CustomActionAck {
    #[sioc(flatten)]
    pub values: Vec<Value>,
}

/// Retains invite-request response arguments ignored by the browser callback.
#[derive(Debug, AckType, DeserializePayload)]
pub struct RespondPartyInviteRequestAck {
    #[sioc(flatten)]
    pub values: Vec<Value>,
}

/// Describes a friend in a social list or an action result.
///
/// List placement can supply online status, so list entries need no `online` key.
#[derive(Debug, Clone, Deserialize)]
pub struct Friend {
    pub username: String,
    pub online: Option<bool>,
    #[serde(rename = "inParty")]
    pub in_party: Option<bool>,
    /// Retains the presence object and its contextual extension fields.
    pub presence: Option<Value>,
}

/// Identifies a pending friend request or blocked player by username.
#[derive(Debug, Clone, Deserialize)]
pub struct FriendRequest {
    pub username: String,
}

/// Contains the social lists consumed by the friends panel.
///
/// Lists preserve omitted or null values. The browser normalizes either to an
/// empty list before updating its social state.
#[derive(Debug, Clone, Deserialize)]
pub struct LoadSocialDataAckInner {
    #[serde(default, rename = "onlineFriends")]
    pub online_friends: Option<Vec<Friend>>,
    #[serde(default, rename = "offlineFriends")]
    pub offline_friends: Option<Vec<Friend>>,
    #[serde(default, rename = "incomingRequests")]
    pub incoming_requests: Option<Vec<FriendRequest>>,
    #[serde(default, rename = "outgoingRequests")]
    pub outgoing_requests: Option<Vec<FriendRequest>>,
    #[serde(default, rename = "blockedPlayers")]
    pub blocked_players: Option<Vec<FriendRequest>>,
    pub error: Option<String>,
}

/// Returns one social-data object.
#[derive(Debug, AckType, DeserializePayload)]
pub struct LoadSocialDataAck {
    pub data: LoadSocialDataAckInner,
}

/// Reports a sent request or an immediately accepted reciprocal friendship.
#[derive(Debug, Clone, Deserialize)]
pub struct SendFriendRequestAckInner {
    #[serde(rename = "friendAdded")]
    pub friend_added: Option<Friend>,
    pub success: Option<bool>,
    /// Describes a rejected request when supplied by the server.
    pub reason: Option<String>,
    pub error: Option<String>,
}

/// Returns the result of sending a friend request.
#[derive(Debug, AckType, DeserializePayload)]
pub struct SendFriendRequestAck {
    pub data: SendFriendRequestAckInner,
}

/// Reports acceptance and the resulting friend when successful.
#[derive(Debug, Clone, Deserialize)]
pub struct AcceptFriendRequestAckInner {
    pub success: Option<bool>,
    pub friend: Option<Friend>,
    pub reason: Option<String>,
    pub error: Option<String>,
}

/// Returns the result of accepting a friend request.
#[derive(Debug, AckType, DeserializePayload)]
pub struct AcceptFriendRequestAck {
    pub data: AcceptFriendRequestAckInner,
}

/// Reports a social or party action's success and optional rejection reason.
#[derive(Debug, Clone, Deserialize)]
pub struct GenericSuccessAckInner {
    pub success: Option<bool>,
    pub reason: Option<String>,
    pub error: Option<String>,
    /// Identifies the invited party when returned by `invite_to_party`.
    pub party_id: Option<String>,
}

/// Returns one result object for a social or party action.
#[derive(Debug, AckType, DeserializePayload)]
pub struct GenericSuccessAck {
    pub data: GenericSuccessAckInner,
}

/// Contains a leaderboard's parallel user and rating arrays.
#[derive(Debug, Clone, Deserialize)]
pub struct LeaderboardData {
    /// Identifies the ladder, including its optional `-alltime` suffix.
    pub ladder: String,
    /// Contains one username per solo entry or two per 2v2 entry.
    pub users: Vec<String>,
    pub stars: Vec<f64>,
    /// Contains supporter markers indexed by username position.
    pub supporters: Vec<bool>,
    /// Contains qualification markers indexed by ranked entry.
    pub qualifiers: Option<Vec<bool>>,
    /// Preserves moderator-only markers whose value domain is unverified.
    pub f: Option<Vec<Value>>,
}

/// Returns a leaderboard object, or null when no result is available.
#[derive(Debug, AckType, DeserializePayload)]
pub struct LeaderboardAck {
    pub data: Option<LeaderboardData>,
}

/// Returns notification values keyed by notification identifier.
///
/// The client reads and removes entries by key. Value schemas remain opaque.
#[derive(Debug, AckType, DeserializePayload)]
pub struct GetNotifsAck {
    pub notifications: Map<String, Value>,
}

/// Returns the current season identifier without assuming its scalar schema.
#[derive(Debug, AckType, DeserializePayload)]
pub struct GetSeasonAck {
    pub season: Value,
}

/// Returns queue counts and highest-player ratings in a positional numeric array.
///
/// The array is one acknowledgement argument. Its indexes are deployment-specific.
#[derive(Debug, AckType, DeserializePayload)]
pub struct QueueCountAck {
    pub counts: Vec<f64>,
}
