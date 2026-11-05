//! Verifies subagent model identity and history across turns on one transport.
#[path = "support/fake_provider.rs"]
mod fake_provider;
#[path = "support/fixtures.rs"]
mod fixtures;

use cursor_server::{
    cursor::{
        prompting::{PromptAssets, PromptCompiler},
        protocol::{connect, proto::agent::v1 as pb},
        tools::{runtime::ExecContext, subagent_models::ByokModel},
        TransportCommand, TransportRegistry,
    },
    model::{Role, ToolCall},
    provider::{FinishReason, ModelEvent},
};
use prost::Message;
use std::sync::Arc;

#[test]
fn ambiguous_model_names_require_a_hash() {
    let context = ExecContext {
        byok_models: vec![
            ByokModel {
                hash: "provider-a".into(),
                model_id: "gpt-x".into(),
                display_name: "GPT X via A".into(),
            },
            ByokModel {
                hash: "provider-b".into(),
                model_id: "other-id".into(),
                display_name: "gpt-x".into(),
            },
        ],
        ..Default::default()
    };
    let arguments = serde_json::json!({"model": "gpt-x"});
    let call = ToolCall {
        index: 0,
        call_id: "task".into(),
        model_call_id: "model".into(),
        name: "Task".into(),
        arguments_text: arguments.to_string(),
        arguments,
        argument_error: None,
    };
    let error = context
        .prepare_call(&call)
        .expect_err("ambiguous aliases must not select the first model");
    assert!(error.to_string().contains("ambiguous"));
}

fn run(text: &str, message_id: &str) -> pb::AgentClientMessage {
    pb::AgentClientMessage {
        message: Some(pb::agent_client_message::Message::RunRequest(
            pb::AgentRunRequest {
                conversation_id: Some("review-conversation".into()),
                requested_model: Some(pb::RequestedModel {
                    model_id: "test-model".into(),
                    ..Default::default()
                }),
                subagent_type_name: Some("generalPurpose".into()),
                action: Some(pb::ConversationAction {
                    action: Some(pb::conversation_action::Action::UserMessageAction(
                        pb::UserMessageAction {
                            user_message: Some(pb::UserMessage {
                                text: text.into(),
                                message_id: message_id.into(),
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
    }
}

async fn assert_subagent_history(message_id: &str, follow_up_action: bool) {
    let (_directory, store) = fixtures::temp_store().await;
    let provider = fake_provider::FakeProvider::default();
    for (id, text) in [
        ("call-one", "first-answer"),
        ("call-two", "second-answer"),
        ("call-three", "third-answer"),
    ] {
        provider.push(vec![
            ModelEvent::Start {
                model_call_id: id.into(),
            },
            ModelEvent::TextStart,
            ModelEvent::TextDelta(text.into()),
            ModelEvent::TextEnd,
            ModelEvent::Done(FinishReason::Stop),
        ]);
    }
    let assets =
        PromptAssets::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("prompt/cursor"))
            .unwrap();
    let registry = TransportRegistry::new(
        store,
        Arc::new(provider.clone()),
        PromptCompiler::new(assets),
    );
    let mut handle = registry.get_or_create("same-transport").await.unwrap();
    let mut output = handle.subscribe();
    let first = run("first-question", message_id);
    let mut seqno = 0;
    let mut turns = 0;
    let mut checkpoint = None;
    handle
        .command(TransportCommand::Append {
            seqno,
            message: Box::new(first.clone()),
        })
        .await
        .unwrap();
    seqno += 1;
    // Replayed Bidi appends must not become a second user input.
    handle
        .command(TransportCommand::Append {
            seqno: 0,
            message: Box::new(first),
        })
        .await
        .unwrap();
    loop {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(5), output.recv())
            .await
            .unwrap()
            .unwrap();
        let (flags, payload) = connect::decode_frames(&frame).unwrap().pop().unwrap();
        if flags & connect::END_STREAM_FLAG != 0 {
            assert_eq!(payload.as_ref(), b"{}");
            if turns == 3 {
                break;
            }
            assert_eq!(turns, 2);
            // Resume from the published checkpoint on a new transport. This
            // must preserve the identities assigned to the earlier inputs.
            handle = registry.get_or_create("resumed-transport").await.unwrap();
            output = handle.subscribe();
            let mut next = run("third-question", "");
            let Some(pb::agent_client_message::Message::RunRequest(request)) =
                next.message.as_mut()
            else {
                unreachable!()
            };
            request.conversation_state = Some(checkpoint.take().unwrap());
            handle
                .command(TransportCommand::Append {
                    seqno: 0,
                    message: Box::new(next),
                })
                .await
                .unwrap();
            seqno = 1;
            continue;
        }
        let message = pb::AgentServerMessage::decode(payload).unwrap();
        match message.message {
            Some(pb::agent_server_message::Message::KvServerMessage(kv)) => {
                let ack = pb::AgentClientMessage {
                    message: Some(pb::agent_client_message::Message::KvClientMessage(
                        pb::KvClientMessage {
                            id: kv.id,
                            message: Some(pb::kv_client_message::Message::SetBlobResult(
                                pb::SetBlobResult { error: None },
                            )),
                        },
                    )),
                };
                handle
                    .command(TransportCommand::Append {
                        seqno,
                        message: Box::new(ack),
                    })
                    .await
                    .unwrap();
                seqno += 1;
            }
            Some(pb::agent_server_message::Message::InteractionUpdate(update))
                if matches!(
                    update.message,
                    Some(pb::interaction_update::Message::TurnEnded(_))
                ) =>
            {
                turns += 1;
                if turns == 1 {
                    let next_id = if message_id.is_empty() {
                        ""
                    } else {
                        "second-user"
                    };
                    let mut next = run("second-question", next_id);
                    if follow_up_action {
                        let Some(pb::agent_client_message::Message::RunRequest(request)) =
                            next.message
                        else {
                            unreachable!()
                        };
                        next.message = Some(pb::agent_client_message::Message::ConversationAction(
                            request.action.unwrap(),
                        ));
                    }
                    handle
                        .command(TransportCommand::Append {
                            seqno,
                            message: Box::new(next),
                        })
                        .await
                        .unwrap();
                    seqno += 1;
                }
            }
            Some(pb::agent_server_message::Message::ConversationCheckpointUpdate(state)) => {
                checkpoint = Some(state);
            }
            _ => {}
        }
    }
    let requests = provider.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        &requests[1].history[..requests[0].history.len()],
        requests[0].history.as_slice()
    );
    assert_eq!(requests[0].prompt, requests[1].prompt);
    assert!(requests[2].history.len() > requests[1].history.len());
    for (original, resumed) in requests[1].history.iter().zip(&requests[2].history) {
        assert_eq!(original.role, resumed.role);
        assert_eq!(original.content, resumed.content);
        // Checkpoint hydration can assign new internal assistant IDs. User
        // input identities and all provider-visible content must stay intact.
        if original.role == Role::User {
            assert_eq!(original.message_id, resumed.message_id);
        }
    }
    assert_eq!(requests[1].prompt, requests[2].prompt);
    let second_history = serde_json::to_string(&requests[1].history).unwrap();
    assert!(
        second_history.contains("first-question"),
        "lost first question: {second_history}"
    );
    assert!(
        second_history.contains("first-answer"),
        "lost first answer: {second_history}"
    );
    assert!(
        second_history.contains("second-question"),
        "lost second question: {second_history}"
    );
}

#[tokio::test]
async fn distinct_user_ids_preserve_history_on_one_transport() {
    assert_subagent_history("first-user", false).await;
}

#[tokio::test]
async fn missing_user_ids_preserve_history_on_one_transport() {
    assert_subagent_history("", false).await;
}

#[tokio::test]
async fn missing_user_id_in_a_follow_up_action_preserves_history() {
    assert_subagent_history("", true).await;
}
