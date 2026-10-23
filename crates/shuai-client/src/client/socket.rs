//! Typed protocol send methods.
//!
//! Keeps request signatures and acknowledgement waits together around one
//! connection owner for positional API audits.

use crate::prelude::*;
use sioc::error::Result;
use sioc::prelude::*;
use std::time::Duration;

/// Sends typed events through one connection with a configurable ack timeout.
#[derive(Debug, Clone)]
pub struct GeneralsIoSender {
    tx: SocketSender,
    ack_timeout: Duration,
}

#[expect(
    clippy::missing_errors_doc,
    reason = "protocol wrappers propagate the underlying Socket.IO transport and acknowledgement errors"
)]
impl GeneralsIoSender {
    /// Wraps a socket sender with a five-second acknowledgement timeout.
    #[must_use]
    pub fn new(tx: SocketSender) -> Self {
        Self {
            tx,
            ack_timeout: Duration::from_secs(5),
        }
    }

    /// Sets the timeout used by acknowledgement-bearing requests.
    #[must_use]
    pub fn with_ack_timeout(mut self, timeout: Duration) -> Self {
        self.ack_timeout = timeout;
        self
    }

    // Account

    /// Sends [`GetUsername`] and waits for its acknowledgement within the configured timeout.
    pub async fn get_username(&self, user_id: String) -> Result<Option<String>> {
        let ack = self
            .tx
            .emit(GetUsername { user_id })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.username)
    }

    /// Sends [`SetUsername`] using the configured connection.
    pub async fn set_username(&self, user_id: String, username: String) -> Result<()> {
        self.tx
            .emit(SetUsername {
                user_id,
                username,
                force_flag: ForceFlag,
                reserved: (),
            })
            .await?;
        Ok(())
    }

    /// Sends [`IsSupporter`] and waits for its acknowledgement within the configured timeout.
    pub async fn is_supporter(&self, user_id: String) -> Result<bool> {
        let ack = self
            .tx
            .emit(IsSupporter { user_id })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.is_supporter)
    }

    /// Sends [`CheckModeration`] and waits for its acknowledgement within the configured timeout.
    pub async fn check_moderation(&self, user_id: String) -> Result<CheckModerationAck> {
        let ack = self
            .tx
            .emit(CheckModeration { user_id })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload)
    }

    /// Sends [`LinkEmail`] using the configured connection.
    pub async fn link_email(&self, user_id: String, email: String, token: String) -> Result<()> {
        self.tx
            .emit(LinkEmail {
                user_id,
                email,
                token,
            })
            .await?;
        Ok(())
    }

    /// Sends [`RecoverAccount`] using the configured connection.
    pub async fn recover_account(&self, email: String, token: String) -> Result<()> {
        self.tx.emit(RecoverAccount { email, token }).await?;
        Ok(())
    }

    // Queue

    /// Sends [`Play`] using the configured connection.
    pub async fn play(&self, user_id: String, color: Option<i32>) -> Result<()> {
        self.tx
            .emit(Play {
                user_id,
                force_flag: ForceFlag,
                color,
                reserved: (),
            })
            .await?;
        Ok(())
    }

    /// Sends [`Join1v1`] using the configured connection.
    pub async fn join_1v1(
        &self,
        user_id: String,
        color: Option<i32>,
        allow_bots: bool,
    ) -> Result<()> {
        self.tx
            .emit(Join1v1 {
                user_id,
                force_flag: ForceFlag,
                color,
                reserved: (),
                allow_bots,
            })
            .await?;
        Ok(())
    }

    /// Sends [`JoinTeam`] using the configured connection.
    pub async fn join_team(
        &self,
        team_id: Option<String>,
        user_id: String,
        color: Option<i32>,
    ) -> Result<()> {
        self.tx
            .emit(JoinTeam {
                team_id,
                user_id,
                force_flag: ForceFlag,
                color,
                reserved: (),
            })
            .await?;
        Ok(())
    }

    /// Sends [`LeaveTeam`] using the configured connection.
    pub async fn leave_team(&self, team_id: Option<String>) -> Result<()> {
        self.tx.emit(LeaveTeam { team_id }).await?;
        Ok(())
    }

    /// Sends [`PlayBigTeam`] using the configured connection.
    pub async fn play_big_team(&self, user_id: String, color: Option<i32>) -> Result<()> {
        self.tx
            .emit(PlayBigTeam {
                user_id,
                force_flag: ForceFlag,
                color,
                reserved: (),
            })
            .await?;
        Ok(())
    }

    /// Sends [`JoinBigTeam`] using the configured connection.
    pub async fn join_big_team(
        &self,
        team_id: Option<String>,
        user_id: String,
        color: Option<i32>,
    ) -> Result<()> {
        self.tx
            .emit(JoinBigTeam {
                team_id,
                user_id,
                force_flag: ForceFlag,
                color,
                reserved: (),
            })
            .await?;
        Ok(())
    }

    /// Sends [`JoinPrivate`] using the configured connection.
    pub async fn join_private(&self, queue_id: String, user_id: String) -> Result<()> {
        self.tx
            .emit(JoinPrivate {
                queue_id,
                user_id,
                force_flag: ForceFlag,
                reserved: (),
            })
            .await?;
        Ok(())
    }

    /// Sends [`Cancel`] using the configured connection.
    pub async fn cancel(&self) -> Result<()> {
        self.tx.emit(Cancel).await?;
        Ok(())
    }

    /// Sends [`SetForceStart`] using the configured connection.
    pub async fn set_force_start(&self, queue_id: String, enabled: bool) -> Result<()> {
        self.tx.emit(SetForceStart { queue_id, enabled }).await?;
        Ok(())
    }

    /// Sends [`QueueCount`] and waits for its acknowledgement within the configured timeout.
    pub async fn queue_count(&self) -> Result<Vec<f64>> {
        let ack = self
            .tx
            .emit(QueueCount)
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.counts)
    }

    /// Sends [`LeaveGame`] using the configured connection.
    pub async fn leave_game(&self) -> Result<()> {
        self.tx.emit(LeaveGame).await?;
        Ok(())
    }

    // Game actions

    /// Sends [`Attack`] using the configured connection.
    pub async fn attack(
        &self,
        from_index: i32,
        to_index: i32,
        is_50_percent: bool,
        attack_index: i32,
    ) -> Result<()> {
        self.tx
            .emit(Attack {
                from_index,
                to_index,
                is_50_percent,
                attack_index,
            })
            .await?;
        Ok(())
    }

    /// Sends [`PingTile`] using the configured connection.
    pub async fn ping_tile(&self, tile_index: i32) -> Result<()> {
        self.tx.emit(PingTile { tile_index }).await?;
        Ok(())
    }

    /// Sends [`Surrender`] using the configured connection.
    pub async fn surrender(&self) -> Result<()> {
        self.tx.emit(Surrender).await?;
        Ok(())
    }

    /// Sends [`UndoMove`] using the configured connection.
    pub async fn undo_move(&self) -> Result<()> {
        self.tx.emit(UndoMove).await?;
        Ok(())
    }

    /// Sends [`ClearMoves`] using the configured connection.
    pub async fn clear_moves(&self) -> Result<()> {
        self.tx.emit(ClearMoves).await?;
        Ok(())
    }

    /// Sends [`Rematch`] using the configured connection.
    pub async fn rematch(&self, wants_rematch: bool) -> Result<()> {
        self.tx.emit(Rematch { wants_rematch }).await?;
        Ok(())
    }

    /// Sends [`StayParty`] using the configured connection.
    pub async fn stay_party(&self, wants_stay: bool) -> Result<()> {
        self.tx.emit(StayParty { wants_stay }).await?;
        Ok(())
    }

    /// Sends [`GameOverState`] using the configured connection.
    pub async fn game_over_state(&self) -> Result<()> {
        self.tx.emit(GameOverState).await?;
        Ok(())
    }

    // Chat

    /// Sends [`EmitChatMessage`] using the configured connection.
    pub async fn chat_message(
        &self,
        chat_room: String,
        text: String,
        prefix: Option<String>,
    ) -> Result<()> {
        self.tx
            .emit(EmitChatMessage {
                chat_room,
                text,
                prefix,
            })
            .await?;
        Ok(())
    }

    /// Sends [`JoinMainMenuChat`] using the configured connection.
    pub async fn join_main_menu_chat(&self, user_id: String) -> Result<()> {
        self.tx.emit(JoinMainMenuChat { user_id }).await?;
        Ok(())
    }

    /// Sends [`LeaveMainMenuChat`] using the configured connection.
    pub async fn leave_main_menu_chat(&self) -> Result<()> {
        self.tx.emit(LeaveMainMenuChat).await?;
        Ok(())
    }

    // Custom games

    /// Sends [`SetCustomOptions`] and waits for its acknowledgement within the configured timeout.
    pub async fn set_custom_options(
        &self,
        game_id: String,
        options: CustomGameOptions,
    ) -> Result<CustomActionAck> {
        let ack = self
            .tx
            .emit(SetCustomOptions { game_id, options })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload)
    }

    /// Sends [`MakeCustomPublic`] and waits for its acknowledgement within the configured timeout.
    pub async fn make_custom_public(&self, game_id: String) -> Result<CustomActionAck> {
        let ack = self
            .tx
            .emit(MakeCustomPublic { game_id })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload)
    }

    /// Sends [`MakeCustomPrivate`] and waits for its acknowledgement within the configured timeout.
    pub async fn make_custom_private(&self, game_id: String) -> Result<CustomActionAck> {
        let ack = self
            .tx
            .emit(MakeCustomPrivate { game_id })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload)
    }

    /// Sends [`UpdateCustomChatRecording`] using the configured connection.
    pub async fn update_custom_chat_recording(
        &self,
        game_id: String,
        disabled: bool,
    ) -> Result<()> {
        self.tx
            .emit(UpdateCustomChatRecording {
                game_id,
                reserved: (),
                disabled,
            })
            .await?;
        Ok(())
    }

    /// Sends [`SetCustomHost`] and waits for its acknowledgement within the configured timeout.
    pub async fn set_custom_host(
        &self,
        game_id: String,
        lobby_index: i32,
    ) -> Result<CustomActionAck> {
        let ack = self
            .tx
            .emit(SetCustomHost {
                game_id,
                lobby_index,
            })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload)
    }

    /// Sends [`KickFromCustom`] and waits for its acknowledgement within the configured timeout.
    pub async fn kick_from_custom(
        &self,
        game_id: String,
        lobby_index: i32,
    ) -> Result<CustomActionAck> {
        let ack = self
            .tx
            .emit(KickFromCustom {
                game_id,
                lobby_index,
            })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload)
    }

    /// Sends [`UnkickFromCustom`] and waits for its acknowledgement within the configured timeout.
    pub async fn unkick_from_custom(
        &self,
        game_id: String,
        username: String,
    ) -> Result<CustomActionAck> {
        let ack = self
            .tx
            .emit(UnkickFromCustom { game_id, username })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload)
    }

    /// Sends [`SetCustomTeam`] using the configured connection.
    pub async fn set_custom_team(&self, game_id: String, team: i32) -> Result<()> {
        self.tx.emit(SetCustomTeam { game_id, team }).await?;
        Ok(())
    }

    /// Sends [`SetColor`] and waits for its acknowledgement within the configured timeout.
    pub async fn set_color(&self, game_id: String, color: i32) -> Result<CustomActionAck> {
        let ack = self
            .tx
            .emit(SetColor { game_id, color })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload)
    }

    /// Sends [`ListenPublicCustoms`] using the configured connection.
    pub async fn listen_public_customs(&self) -> Result<()> {
        self.tx.emit(ListenPublicCustoms).await?;
        Ok(())
    }

    /// Sends [`StopListenPublicCustoms`] using the configured connection.
    pub async fn stop_listen_public_customs(&self) -> Result<()> {
        self.tx.emit(StopListenPublicCustoms).await?;
        Ok(())
    }

    // Party

    /// Sends [`JoinParty`] using the configured connection.
    pub async fn join_party(&self, party_id: Option<String>, user_id: String) -> Result<()> {
        self.tx.emit(JoinParty { party_id, user_id }).await?;
        Ok(())
    }

    /// Sends [`LeaveParty`] using the configured connection.
    pub async fn leave_party(&self) -> Result<()> {
        self.tx.emit(LeaveParty).await?;
        Ok(())
    }

    /// Sends [`InviteToParty`] and waits for its acknowledgement within the configured timeout.
    pub async fn invite_to_party(&self, username: String) -> Result<GenericSuccessAckInner> {
        let ack = self
            .tx
            .emit(InviteToParty { username })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    /// Sends [`RequestPartyInvite`] and waits for its acknowledgement within the configured timeout.
    pub async fn request_party_invite(&self, username: String) -> Result<GenericSuccessAckInner> {
        let ack = self
            .tx
            .emit(RequestPartyInvite { username })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    /// Sends [`RespondPartyInviteRequest`] and waits for its acknowledgement within the configured timeout.
    pub async fn respond_party_invite_request(
        &self,
        username: String,
        accepted: bool,
    ) -> Result<RespondPartyInviteRequestAck> {
        let ack = self
            .tx
            .emit(RespondPartyInviteRequest { username, accepted })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload)
    }

    // Friends

    /// Sends [`LoadSocialData`] and waits for its acknowledgement within the configured timeout.
    pub async fn load_social_data(&self, user_id: String) -> Result<LoadSocialDataAckInner> {
        let ack = self
            .tx
            .emit(LoadSocialData { user_id })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    /// Sends [`SendFriendRequest`] and waits for its acknowledgement within the configured timeout.
    pub async fn send_friend_request(
        &self,
        user_id: String,
        username: String,
    ) -> Result<SendFriendRequestAckInner> {
        let ack = self
            .tx
            .emit(SendFriendRequest { user_id, username })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    /// Sends [`AcceptFriendRequest`] and waits for its acknowledgement within the configured timeout.
    pub async fn accept_friend_request(
        &self,
        username: String,
    ) -> Result<AcceptFriendRequestAckInner> {
        let ack = self
            .tx
            .emit(AcceptFriendRequest { username })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    /// Sends [`DeclineFriendRequest`] and waits for its acknowledgement within the configured timeout.
    pub async fn decline_friend_request(&self, username: String) -> Result<GenericSuccessAckInner> {
        let ack = self
            .tx
            .emit(DeclineFriendRequest { username })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    /// Sends [`CancelFriendRequest`] and waits for its acknowledgement within the configured timeout.
    pub async fn cancel_friend_request(&self, username: String) -> Result<GenericSuccessAckInner> {
        let ack = self
            .tx
            .emit(CancelFriendRequest { username })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    /// Sends [`RemoveFriend`] and waits for its acknowledgement within the configured timeout.
    pub async fn remove_friend(&self, username: String) -> Result<GenericSuccessAckInner> {
        let ack = self
            .tx
            .emit(RemoveFriend { username })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    // Stats

    /// Sends [`Get2v2Teammates`] using the configured connection.
    pub async fn get_2v2_teammates(&self, user_id: String, all_time: bool) -> Result<()> {
        self.tx.emit(Get2v2Teammates { user_id, all_time }).await?;
        Ok(())
    }

    /// Sends [`Leaderboard`] and waits for its acknowledgement within the configured timeout.
    pub async fn leaderboard(
        &self,
        leaderboard_id: String,
        user_id: String,
    ) -> Result<Option<LeaderboardData>> {
        let ack = self
            .tx
            .emit(Leaderboard {
                leaderboard_id,
                user_id,
            })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    /// Sends [`GetNotifs`] and waits for its acknowledgement within the configured timeout.
    pub async fn get_notifs(
        &self,
        user_id: String,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let ack = self
            .tx
            .emit(GetNotifs { user_id })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.notifications)
    }

    /// Sends [`ClearNotif`] using the configured connection.
    pub async fn clear_notif(&self, user_id: String, notif_id: String) -> Result<()> {
        self.tx.emit(ClearNotif { user_id, notif_id }).await?;
        Ok(())
    }

    /// Sends [`GetSeason`] and waits for its acknowledgement within the configured timeout.
    pub async fn get_season(&self) -> Result<serde_json::Value> {
        let ack = self
            .tx
            .emit(GetSeason)
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.season)
    }

    /// Sends [`StarsAndRank`] using the configured connection.
    pub async fn stars_and_rank(&self, user_id: String) -> Result<()> {
        self.tx.emit(StarsAndRank { user_id }).await?;
        Ok(())
    }

    /// Sends [`MainMenuActivity`] using the configured connection.
    pub async fn main_menu_activity(&self) -> Result<()> {
        self.tx.emit(MainMenuActivity).await?;
        Ok(())
    }

    // Connectivity

    /// Sends [`PingServer`] using the configured connection.
    pub async fn ping_server(&self) -> Result<()> {
        self.tx.emit(PingServer).await?;
        Ok(())
    }

    /// Sends [`PingWorker`] using the configured connection.
    pub async fn ping_worker(&self) -> Result<()> {
        self.tx.emit(PingWorker).await?;
        Ok(())
    }

    /// Sends [`KickFromParty`] and waits for its acknowledgement within the configured timeout.
    pub async fn kick_from_party(&self, username: String) -> Result<GenericSuccessAckInner> {
        let ack = self
            .tx
            .emit(KickFromParty { username })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    /// Sends [`BlockSocial`] and waits for its acknowledgement within the configured timeout.
    pub async fn block_social(&self, username: String) -> Result<GenericSuccessAckInner> {
        let ack = self
            .tx
            .emit(BlockSocial { username })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    /// Sends [`UnblockSocial`] and waits for its acknowledgement within the configured timeout.
    pub async fn unblock_social(&self, username: String) -> Result<GenericSuccessAckInner> {
        let ack = self
            .tx
            .emit(UnblockSocial { username })
            .await?
            .timeout(self.ack_timeout)
            .await?;
        Ok(ack.payload.data)
    }

    /// Sends [`StartHoldingClearMoves`] using the configured connection.
    pub async fn start_holding_clear_moves(&self) -> Result<()> {
        self.tx.emit(StartHoldingClearMoves).await?;
        Ok(())
    }

    /// Sends [`StopHoldingClearMoves`] using the configured connection.
    pub async fn stop_holding_clear_moves(&self) -> Result<()> {
        self.tx.emit(StopHoldingClearMoves).await?;
        Ok(())
    }

    /// Sends [`ModerateRemoveChatMessage`] using the configured connection.
    pub async fn moderate_remove_chat_message(
        &self,
        chat_room: String,
        message: ModeratedChatMessage,
    ) -> Result<()> {
        self.tx
            .emit(ModerateRemoveChatMessage { chat_room, message })
            .await?;
        Ok(())
    }

    /// Sends [`ModerateRedactPlayerChat`] using the configured connection.
    pub async fn moderate_redact_player_chat(
        &self,
        chat_room: String,
        username: String,
    ) -> Result<()> {
        self.tx
            .emit(ModerateRedactPlayerChat {
                chat_room,
                username,
            })
            .await?;
        Ok(())
    }
}

impl std::ops::Deref for GeneralsIoSender {
    type Target = SocketSender;

    fn deref(&self) -> &SocketSender {
        &self.tx
    }
}
