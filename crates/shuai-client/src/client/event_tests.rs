use crate::prelude::*;
use sioc::prelude::{DeserializePayload, EventType, ack_from_json, event_from_json, event_to_json};

#[derive(EventType, DeserializePayload)]
#[sioc(event(name = "option_probe"))]
struct OptionProbe {
    value: Option<bool>,
}

#[test]
fn holding_clear_moves_events_have_no_arguments() {
    assert_eq!(
        event_to_json(&StartHoldingClearMoves).unwrap(),
        r#"["start_holding_clear_moves"]"#
    );
    assert_eq!(
        event_to_json(&StopHoldingClearMoves).unwrap(),
        r#"["stop_holding_clear_moves"]"#
    );
}

#[test]
fn join_1v1_keeps_five_ordered_arguments_and_null_reserved_slot() {
    let event = Join1v1 {
        user_id: "user".into(),
        force_flag: ForceFlag,
        options: None,
        reserved: (),
        allow_bots: true,
    };
    assert_eq!(
        event_to_json(&event).unwrap(),
        r#"["join_1v1","user","sd09fjdZ03i0ejwi_changeme",null,null,true]"#
    );
}

#[test]
fn check_moderation_ack_has_three_positional_fields() {
    let ack: CheckModerationAck = ack_from_json(r#"[true,false,"warning"]"#).unwrap();
    assert!(ack.is_muted);
    assert!(!ack.is_disabled);
    assert_eq!(ack.warning.as_deref(), Some("warning"));
}

#[test]
fn stay_party_update_flattens_all_slots_and_requires_no_fixed_slots() {
    for (payload, expected) in [
        (r#"["stay_party_update"]"#, vec![]),
        (r#"["stay_party_update",1]"#, vec![serde_json::json!(1)]),
        (
            r#"["stay_party_update",1,"x",null]"#,
            vec![
                serde_json::json!(1),
                serde_json::json!("x"),
                serde_json::Value::Null,
            ],
        ),
    ] {
        let event: StayPartyUpdate = event_from_json(payload).unwrap();
        assert_eq!(event.data, expected);
    }
}

#[test]
fn missing_option_slot_is_an_error_but_null_is_present() {
    assert!(event_from_json::<OptionProbe>(r#"["option_probe"]"#).is_err());
    let event: OptionProbe = event_from_json(r#"["option_probe",null]"#).unwrap();
    assert_eq!(event.value, None);
}
