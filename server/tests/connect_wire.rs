//! Verifies captured Cursor Connect framing and protobuf compatibility.
#[path = "support/fake_cursor.rs"]
mod fake_cursor;
#[path = "support/fake_provider.rs"]
mod fake_provider;
#[path = "support/fixtures.rs"]
mod fixtures;

use std::{io::Write, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use cursor_server::{
    api::cursor,
    cursor::prompting::{PromptAssets, PromptCompiler},
    cursor::protocol::{
        connect,
        proto::{agent::v1 as pb, aiserver::v1 as ai},
    },
    cursor::transport::TransportRegistry,
    network::NetworkClients,
};
use flate2::{write::GzEncoder, Compression};
use prost::Message;
use tower::ServiceExt;

#[test]
fn connect_envelope_is_flag_plus_big_endian_length_plus_protobuf() {
    let message = pb::BidiRequestId {
        request_id: "abc".into(),
    };
    let frame = connect::encode_message(&message).unwrap();
    assert_eq!(&frame[..5], &[0, 0, 0, 0, 5]);
    let decoded: pb::BidiRequestId = fake_cursor::decode_single(&frame).unwrap();
    assert_eq!(decoded.request_id, "abc");
}

#[test]
fn end_stream_matches_captured_connect_shape() {
    assert_eq!(
        connect::encode_end_stream().as_ref(),
        &[2, 0, 0, 0, 2, b'{', b'}']
    );
}

#[test]
fn error_end_stream_is_flagged_json_not_protobuf() {
    let frame = connect::encode_error_end_stream(&connect::ConnectStreamError {
        code: connect::ConnectCode::Unavailable,
        message: "overloaded".into(),
        details: vec![connect::ConnectErrorDetail {
            type_name: "aiserver.v1.ErrorDetails".into(),
            value: "AQ".into(),
        }],
    })
    .unwrap();
    let (flags, payload) = connect::decode_frames(&frame).unwrap().pop().unwrap();
    assert_eq!(flags, connect::END_STREAM_FLAG);
    let json: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(json["error"]["code"], "unavailable");
    assert_eq!(json["error"]["message"], "overloaded");
    assert_eq!(
        json["error"]["details"][0]["type"],
        "aiserver.v1.ErrorDetails"
    );
}

#[test]
fn cursor_error_details_subset_decodes_captured_wire_value() {
    let captured = "CAISVQoUQXV0aGVudGljYXRpb24gZXJyb3ISMklmIHlvdSBhcmUgbG9nZ2VkIGluLCB0cnkgbG9nZ2luZyBvdXQgYW5kIGJhY2sgaW4uIABSBwoFbG9naW4YAQ";
    let bytes = STANDARD_NO_PAD.decode(captured).unwrap();
    let details = ai::ErrorDetails::decode(bytes.as_slice()).unwrap();
    assert_eq!(details.error, 2, "ERROR_NOT_LOGGED_IN");
    assert_eq!(details.is_expected, Some(true));
    let custom = details.details.unwrap();
    assert_eq!(custom.title, "Authentication error");
    assert_eq!(custom.is_retryable, Some(false));
}

#[test]
fn captured_kv_ack_hex_decodes_as_agent_client_message() {
    let bytes = hex::decode("1a0408011a00").unwrap();
    let message = pb::AgentClientMessage::decode(bytes.as_slice()).unwrap();
    let Some(pb::agent_client_message::Message::KvClientMessage(kv)) = message.message else {
        panic!("expected KV client message")
    };
    assert_eq!(kv.id, 1);
    assert!(matches!(
        kv.message,
        Some(pb::kv_client_message::Message::SetBlobResult(_))
    ));
}

#[tokio::test]
async fn bidi_append_accepts_the_binary_and_hex_forms_from_cursor() {
    let (_directory, store) = fixtures::temp_store().await;
    let assets = PromptAssets::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("prompt/cursor")
            .as_path(),
    )
    .unwrap();
    let clients = NetworkClients::new(store.clone());
    let registry = TransportRegistry::new(
        store,
        Arc::new(fake_provider::FakeProvider::default()),
        PromptCompiler::new(assets),
    );
    let router = cursor::router(registry.clone(), clients).unwrap();
    let heartbeat = pb::AgentClientMessage {
        message: Some(pb::agent_client_message::Message::ClientHeartbeat(
            pb::ClientHeartbeat {},
        )),
    }
    .encode_to_vec();

    for (request_id, data, data_binary) in [
        ("binary", String::new(), heartbeat.clone()),
        ("hex", hex::encode(&heartbeat), Vec::new()),
    ] {
        let handle = registry.get_or_create(request_id).await.unwrap();
        let mut output = handle.subscribe();
        let wire = ai::BidiAppendRequest {
            request_id: Some(ai::BidiRequestId {
                request_id: request_id.into(),
            }),
            append_seqno: 1,
            data,
            data_binary,
        }
        .encode_to_vec();
        let response = router
            .clone()
            .oneshot(
                Request::post("/aiserver.v1.BidiService/BidiAppend")
                    .header(header::CONTENT_TYPE, "application/proto")
                    .body(Body::from(wire))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{request_id} append");

        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), output.recv())
            .await
            .unwrap()
            .unwrap();
        let message: pb::AgentServerMessage = fake_cursor::decode_single(&frame).unwrap();
        assert!(matches!(
            message.message,
            Some(pb::agent_server_message::Message::InteractionUpdate(
                pb::InteractionUpdate {
                    message: Some(pb::interaction_update::Message::Heartbeat(_)),
                }
            ))
        ));
    }
    registry.shutdown().await;
}

#[tokio::test]
async fn bidi_append_gzip_body_is_decompressed_before_protobuf_decode() {
    let (_directory, store) = fixtures::temp_store().await;
    let assets = PromptAssets::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("prompt/cursor")
            .as_path(),
    )
    .unwrap();
    let clients = NetworkClients::new(store.clone());
    let registry = TransportRegistry::new(
        store,
        Arc::new(fake_provider::FakeProvider::default()),
        PromptCompiler::new(assets),
    );
    let wire = ai::BidiAppendRequest {
        request_id: Some(ai::BidiRequestId {
            request_id: "gzip-request".into(),
        }),
        ..Default::default()
    }
    .encode_to_vec();
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&wire).unwrap();
    let compressed = encoder.finish().unwrap();

    let response = cursor::router(registry, clients)
        .unwrap()
        .oneshot(
            Request::post("/aiserver.v1.BidiService/BidiAppend")
                .header(header::CONTENT_TYPE, "application/proto")
                .header(header::CONTENT_ENCODING, "gzip")
                .body(Body::from(compressed))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let text = std::str::from_utf8(&body).unwrap();
    assert!(text.contains("BidiAppend contains no AgentClientMessage"));
    assert!(!text.contains("protobuf decode error"));
}

#[tokio::test]
async fn binary_first_append_routes_a_run_and_streams_protobuf_until_completion() {
    use cursor_server::provider::{FinishReason, ModelEvent};
    use futures_util::StreamExt;

    let (_directory, store) = fixtures::temp_store().await;
    let provider = fake_provider::FakeProvider::default();
    provider.push(vec![
        ModelEvent::Start {
            model_call_id: "binary-call".into(),
        },
        ModelEvent::TextStart,
        ModelEvent::TextDelta("binary chat works".into()),
        ModelEvent::TextEnd,
        ModelEvent::Done(FinishReason::Stop),
    ]);
    let assets =
        PromptAssets::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("prompt/cursor"))
            .unwrap();
    let clients = NetworkClients::new(store.clone());
    let registry = TransportRegistry::new(
        store,
        Arc::new(provider.clone()),
        PromptCompiler::new(assets),
    );
    let router = cursor::router(registry.clone(), clients).unwrap();
    let request_id = "binary-first-append";
    let message = pb::AgentClientMessage {
        message: Some(pb::agent_client_message::Message::RunRequest(
            pb::AgentRunRequest {
                conversation_id: Some("binary-conversation".into()),
                run_id: Some(request_id.into()),
                requested_model: Some(pb::RequestedModel {
                    model_id: "plugin:test/provider/model".into(),
                    ..Default::default()
                }),
                action: Some(pb::ConversationAction {
                    action: Some(pb::conversation_action::Action::UserMessageAction(
                        pb::UserMessageAction {
                            user_message: Some(pb::UserMessage {
                                text: "Hello".into(),
                                message_id: "binary-user".into(),
                                mode: pb::AgentMode::Agent as i32,
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    )),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )),
    };
    binary_append(&router, request_id, 0, message).await;
    assert!(registry.local(request_id).await.is_some());
    let response = router
        .clone()
        .oneshot(
            Request::post("/agent.v1.AgentService/RunSSE")
                .header(header::CONTENT_TYPE, "application/proto")
                .body(Body::from(
                    connect::encode_message(&pb::BidiRequestId {
                        request_id: request_id.into(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    let mut seqno = 1;
    let mut text = String::new();
    loop {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(10), stream.next())
            .await
            .expect("RunSSE response timed out")
            .expect("RunSSE ended before completion")
            .unwrap();
        for (flags, payload) in connect::decode_frames(&frame).unwrap() {
            if flags & connect::END_STREAM_FLAG != 0 {
                let end: serde_json::Value = serde_json::from_slice(&payload).unwrap();
                assert!(end.get("error").is_none(), "{end}");
                assert_eq!(text, "binary chat works");
                assert_eq!(provider.requests().len(), 1);
                registry.shutdown().await;
                return;
            }
            let message = pb::AgentServerMessage::decode(payload).unwrap();
            match message.message {
                Some(pb::agent_server_message::Message::KvServerMessage(kv)) => {
                    binary_append(
                        &router,
                        request_id,
                        seqno,
                        pb::AgentClientMessage {
                            message: Some(pb::agent_client_message::Message::KvClientMessage(
                                pb::KvClientMessage {
                                    id: kv.id,
                                    message: Some(pb::kv_client_message::Message::SetBlobResult(
                                        pb::SetBlobResult { error: None },
                                    )),
                                },
                            )),
                        },
                    )
                    .await;
                    seqno += 1;
                }
                Some(pb::agent_server_message::Message::InteractionUpdate(update)) => {
                    if let Some(pb::interaction_update::Message::TextDelta(delta)) = update.message
                    {
                        text.push_str(&delta.text);
                    }
                }
                _ => {}
            }
        }
    }
}

async fn binary_append(
    router: &axum::Router,
    request_id: &str,
    seqno: i64,
    message: pb::AgentClientMessage,
) {
    let response = router
        .clone()
        .oneshot(
            Request::post("/aiserver.v1.BidiService/BidiAppend")
                .header(header::CONTENT_TYPE, "application/proto")
                .body(Body::from(
                    ai::BidiAppendRequest {
                        request_id: Some(ai::BidiRequestId {
                            request_id: request_id.into(),
                        }),
                        append_seqno: seqno,
                        data_binary: message.encode_to_vec(),
                        ..Default::default()
                    }
                    .encode_to_vec(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
}

#[test]
fn bidi_append_rejects_missing_ambiguous_and_invalid_payloads() {
    let request = ai::BidiAppendRequest {
        request_id: Some(ai::BidiRequestId {
            request_id: "invalid-append".into(),
        }),
        ..Default::default()
    };
    assert!(cursor::bidi::decode(&request).is_err());
    let heartbeat = pb::AgentClientMessage {
        message: Some(pb::agent_client_message::Message::ClientHeartbeat(
            pb::ClientHeartbeat {},
        )),
    }
    .encode_to_vec();
    assert!(cursor::bidi::decode(&ai::BidiAppendRequest {
        data: hex::encode(&heartbeat),
        data_binary: heartbeat,
        ..request.clone()
    })
    .is_err());
    assert!(cursor::bidi::decode(&ai::BidiAppendRequest {
        data_binary: vec![0xff],
        ..request
    })
    .is_err());
}
