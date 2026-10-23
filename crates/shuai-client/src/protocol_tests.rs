use crate::prelude::*;
use serde_json::{Value, json};
use sioc::prelude::{SerializePayload, ack_from_json, event_from_json, event_to_json};

fn assert_wire(
    event: &(impl sioc::prelude::EventType + SerializePayload),
    expected: impl Into<Value>,
) {
    let expected = expected.into();
    let actual: Value = serde_json::from_str(&event_to_json(event).unwrap()).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn queue_requests_send_color_and_explicit_null_slots() {
    let force = serde_json::to_value(ForceFlag).unwrap();
    assert_wire(
        &Play {
            user_id: "u".into(),
            force_flag: ForceFlag,
            color: Some(3),
            reserved: (),
        },
        json!(["play", "u", force, 3, null]),
    );
    assert_wire(
        &PlayBigTeam {
            user_id: "u".into(),
            force_flag: ForceFlag,
            color: None,
            reserved: (),
        },
        json!(["play_big_team", "u", force, null, null]),
    );
    assert_wire(
        &JoinTeam {
            team_id: None,
            user_id: "u".into(),
            force_flag: ForceFlag,
            color: Some(2),
            reserved: (),
        },
        json!(["join_team", null, "u", force, 2, null]),
    );
    assert_wire(
        &JoinBigTeam {
            team_id: Some("team".into()),
            user_id: "u".into(),
            force_flag: ForceFlag,
            color: None,
            reserved: (),
        },
        json!(["join_big_team", "team", "u", force, null, null]),
    );
    assert_wire(
        &JoinPrivate {
            queue_id: "room".into(),
            user_id: "u".into(),
            force_flag: ForceFlag,
            reserved: (),
        },
        json!(["join_private", "room", "u", force, null]),
    );
    assert_wire(
        &SetUsername {
            user_id: "u".into(),
            username: "name".into(),
            force_flag: ForceFlag,
            reserved: (),
        },
        json!(["set_username", "u", "name", force, null]),
    );
    assert_wire(&QueueCount, json!(["queue_count"]));
    assert_wire(&GetSeason, json!(["get_season"]));
    assert_wire(
        &SetForceStart {
            queue_id: "g".into(),
            enabled: false,
        },
        json!(["set_force_start", "g", false]),
    );
}

#[test]
fn custom_requests_exclude_final_callbacks_from_arguments() {
    let options = serde_json::from_value(json!({"game_speed": 1.5, "modifiers": [0, 11]})).unwrap();
    assert_wire(
        &SetCustomOptions {
            game_id: "g".into(),
            options,
        },
        json!(["set_custom_options", "g", {"game_speed":1.5,"modifiers":[0,11]}]),
    );
    assert_wire(
        &MakeCustomPublic {
            game_id: "g".into(),
        },
        json!(["make_custom_public", "g"]),
    );
    assert_wire(
        &MakeCustomPrivate {
            game_id: "g".into(),
        },
        json!(["make_custom_private", "g"]),
    );
    assert_wire(
        &SetCustomHost {
            game_id: "g".into(),
            lobby_index: 2,
        },
        json!(["set_custom_host", "g", 2]),
    );
    assert_wire(
        &KickFromCustom {
            game_id: "g".into(),
            lobby_index: 2,
        },
        json!(["kick_from_custom", "g", 2]),
    );
    assert_wire(
        &UnkickFromCustom {
            game_id: "g".into(),
            username: "name".into(),
        },
        json!(["unkick_from_custom", "g", "name"]),
    );
    assert_wire(
        &SetColor {
            game_id: "g".into(),
            color: 4,
        },
        json!(["set_color", "g", 4]),
    );
    assert_wire(
        &UpdateCustomChatRecording {
            game_id: "g".into(),
            reserved: (),
            disabled: true,
        },
        json!(["update_custom_chat_recording", "g", null, true]),
    );
    let mut options: CustomGameOptions = serde_json::from_value(json!({
        "stronghold_strength_min": 20,
        "stronghold_strength_max": 100
    }))
    .unwrap();
    options.extra.insert("map".into(), Value::Null);
    assert_wire(
        &SetCustomOptions {
            game_id: "g".into(),
            options,
        },
        json!(["set_custom_options", "g", {
            "map": null,
            "stronghold_strength_min": 20,
            "stronghold_strength_max": 100
        }]),
    );
}

#[test]
fn party_and_social_requests_use_usernames_without_party_ids() {
    assert_wire(
        &InviteToParty {
            username: "name".into(),
        },
        json!(["invite_to_party", "name"]),
    );
    assert_wire(
        &RequestPartyInvite {
            username: "name".into(),
        },
        json!(["request_party_invite", "name"]),
    );
    assert_wire(
        &RespondPartyInviteRequest {
            username: "name".into(),
            accepted: true,
        },
        json!(["respond_party_invite_request", "name", true]),
    );
    assert_wire(
        &KickFromParty {
            username: "name".into(),
        },
        json!(["kick_from_party", "name"]),
    );
    assert_wire(
        &BlockSocial {
            username: "name".into(),
        },
        json!(["block_social", "name"]),
    );
    assert_wire(
        &UnblockSocial {
            username: "name".into(),
        },
        json!(["unblock_social", "name"]),
    );
    assert_wire(
        &Leaderboard {
            leaderboard_id: "1v1".into(),
            user_id: "u".into(),
        },
        json!(["leaderboard", "1v1", "u"]),
    );
    assert_wire(
        &Attack {
            from_index: 0,
            to_index: 1,
            is_50_percent: false,
            attack_index: 7,
        },
        json!(["attack", 0, 1, false, 7]),
    );
}

#[test]
fn moderation_requests_preserve_message_matching_fields() {
    assert_wire(
        &ModerateRedactPlayerChat {
            chat_room: "r".into(),
            username: "u".into(),
        },
        json!(["moderate_redact_player_chat", "r", "u"]),
    );
    assert_wire(
        &ModerateRemoveChatMessage {
            chat_room: "r".into(),
            message: ModeratedChatMessage {
                username: Some("u".into()),
                text: "hello".into(),
                prefix: String::new(),
                timestamp: Some(json!(123)),
            },
        },
        json!(["moderate_remove_chat_message", "r", {"username":"u", "text":"hello", "prefix":"", "timestamp":123}]),
    );
}

#[test]
fn acknowledgements_decode_objects_maps_and_fractional_ratings() {
    let leaderboard: LeaderboardAck = ack_from_json(
        r#"[{"ladder":"2v2","users":["a","b"],"stars":[12.5],"supporters":[false,true]}]"#,
    )
    .unwrap();
    assert_eq!(leaderboard.data.unwrap().stars, vec![12.5]);
    assert!(
        ack_from_json::<LeaderboardAck>("[null]")
            .unwrap()
            .data
            .is_none()
    );
    let notifs: GetNotifsAck =
        ack_from_json(r#"[{"reset_ffa":true,"future":{"value":1}}]"#).unwrap();
    assert_eq!(notifs.notifications["reset_ffa"], true);
    let social: LoadSocialDataAck = ack_from_json(r#"[{"blockedPlayers":[{"username":"blocked"}],"incomingRequests":[{"username":"requester"}]}]"#).unwrap();
    assert_eq!(
        social.data.blocked_players.as_ref().unwrap()[0].username,
        "blocked"
    );
    let request: SendFriendRequestAck =
        ack_from_json(r#"[{"success":false,"reason":"blocked"}]"#).unwrap();
    assert_eq!(request.data.reason.as_deref(), Some("blocked"));
    let party: GenericSuccessAck = ack_from_json(r#"[{"success":true,"party_id":"p"}]"#).unwrap();
    assert_eq!(party.data.party_id.as_deref(), Some("p"));
    for payload in [json!([]), json!([null]), json!([{"reason":"denied"}])] {
        let ack: CustomActionAck = ack_from_json(&payload.to_string()).unwrap();
        assert_eq!(ack.values, payload.as_array().unwrap().as_slice());
        let reply: RespondPartyInviteRequestAck = ack_from_json(&payload.to_string()).unwrap();
        assert_eq!(reply.values, ack.values);
    }
    assert_eq!(
        ack_from_json::<QueueCountAck>("[[1,2,3,4,0,0,12.5]]")
            .unwrap()
            .counts
            .len(),
        7
    );
    assert_eq!(
        ack_from_json::<GetSeasonAck>(r#"["season"]"#)
            .unwrap()
            .season,
        "season"
    );
}

#[test]
fn social_lists_and_presence_preserve_nullable_context() {
    let empty: LoadSocialDataAck = ack_from_json(
        r#"[{"onlineFriends":null,"offlineFriends":null,"incomingRequests":null,"outgoingRequests":null,"blockedPlayers":null}]"#,
    ).unwrap();
    assert!(empty.data.online_friends.is_none());
    assert!(empty.data.offline_friends.is_none());
    assert!(empty.data.incoming_requests.is_none());
    assert!(empty.data.outgoing_requests.is_none());
    assert!(empty.data.blocked_players.is_none());
    let social: LoadSocialDataAck =
        ack_from_json(r#"[{"onlineFriends":[{"username":"u","presence":{"state":"main_menu"}}]}]"#)
            .unwrap();
    let friend = &social.data.online_friends.as_ref().unwrap()[0];
    assert!(friend.online.is_none());
    assert_eq!(friend.presence.as_ref().unwrap()["state"], "main_menu");
    let event: FriendPresence = event_from_json(
        r#"["friend_presence",{"username":"u","presence":{"state":"main_menu","future":7}}]"#,
    )
    .unwrap();
    assert_eq!(event.data.presence["state"], "main_menu");
    assert_eq!(event.data.presence["future"], 7);
}

#[test]
fn incoming_party_redaction_and_listing_shapes_match_consumers() {
    let joined: JoinedParty = event_from_json(r#"["joined_party","p",true]"#).unwrap();
    assert!(joined.is_host);
    let party: PartyUpdate = event_from_json(r#"["party_update",{"party_id":"p","host_username":"u","members":[{"username":"u","isHost":true}]}]"#).unwrap();
    assert!(party.data.members[0].is_host);
    let redaction: ChatRedaction =
        event_from_json(r#"["chat_redaction","r",{"redactUsername":true,"username":"u"}]"#)
            .unwrap();
    assert_eq!(redaction.data.redact_username, Some(true));
    let listing: PublicCustomsUpdate = event_from_json(r#"["public_customs_update",{"id":"g","host":"u","map":null,"players":3,"mods":[["Speed",1.5],["Fog","On"]]},false]"#).unwrap();
    assert!(!listing.removed);
    assert_eq!(listing.game.mods.len(), 2);
    let removed: PublicCustomsUpdate =
        event_from_json(r#"["public_customs_update",{"id":"g"},true]"#).unwrap();
    assert!(removed.removed);
    let teammates: Teammates2v2 =
        event_from_json(r#"["2v2_teammates",[{"username":"u","stars":4.5,"rank":2}],true]"#)
            .unwrap();
    assert_eq!(teammates.teammates[0].stars, Some(4.5));
}

#[test]
fn queue_fields_use_wire_casing_and_string_wait_messages() {
    let update: QueueUpdate = event_from_json(r#"["queue_update",{"id":"g","numPlayers":2,"wait":"Waiting for players","kicks":["u"],"bans":[],"future":7}]"#).unwrap();
    assert_eq!(update.data.num_players, Some(2));
    assert_eq!(update.data.wait.as_deref(), Some("Waiting for players"));
    assert_eq!(update.data.kicks.unwrap(), vec!["u"]);
    assert_eq!(update.data.extra["future"], 7);
}

#[test]
fn warning_notifications_accept_omitted_timings_and_ignored_arguments() {
    for (payload, expected) in [
        (json!(["warn_popup", "warning"]), vec![]),
        (json!(["warn_popup", "warning", 250]), vec![Some(250.0)]),
        (
            json!(["warn_popup", "warning", null, 1500.5]),
            vec![None, Some(1500.5)],
        ),
    ] {
        let event: WarnPopup = event_from_json(&payload.to_string()).unwrap();
        assert_eq!(event.warn_message, "warning");
        assert_eq!(event.timing_ms, expected);
    }
    for payload in [
        r#"["moderation_warning"]"#,
        r#"["moderation_warning","ignored"]"#,
    ] {
        event_from_json::<ModerationWarning>(payload).unwrap();
    }
}

#[test]
fn game_payloads_preserve_clock_pings_and_loss_reasons() {
    let start: GameStart = event_from_json(r#"["game_start",{"playerIndex":0,"chat_room":"r","usernames":["u"],"game_type":"custom","options":{},"chessClockConfig":{"clockIncrement":1.5,"startingClock":30,"decayBufferMs":100,"dontLimitMoveSpeed":true},"chessClock":{"playerClocks":[29.5],"runningPlayers":[0],"decayStartsAt":1234}}]"#).unwrap();
    assert_eq!(start.data.chess_clock.unwrap().player_clocks, vec![29.5]);
    let update: GameUpdate = event_from_json(r#"["game_update",{"turn":1,"scores":[{"total":-1,"tiles":-1,"i":0,"color":0,"neutralize_in":0.25,"warn_afk":true}],"attackIndex":0,"generals":[0],"map_diff":[],"cities_diff":[],"stars":[1.25],"uncertainties":[0.5],"pings":[{"tileIndex":0,"playerColor":0,"playerIndex":0}]}]"#).unwrap();
    assert_eq!(update.data.scores[0].neutralize_in, Some(0.25));
    assert!(!update.data.scores[0].has_kill);
    assert_eq!(update.data.pings.unwrap()[0].tile_index, 0);
    let lost: GameLost = event_from_json(r#"["game_lost",{"surrender":true,"afk":true}]"#).unwrap();
    assert!(lost.data.afk);
    assert!(!lost.data.no_general);
    event_from_json::<GameWon>(r#"["game_won"]"#).unwrap();
}
