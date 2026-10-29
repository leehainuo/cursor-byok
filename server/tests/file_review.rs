//! Verifies file review snapshots across edits and checkpoint recovery.
#[path = "support/fake_provider.rs"]
mod fake_provider;
#[path = "support/fixtures.rs"]
mod fixtures;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use cursor_server::{
    cursor::{
        prompting::{PromptAssets, PromptCompiler},
        protocol::{connect, proto::agent::v1 as pb},
        TransportCommand, TransportRegistry,
    },
    provider::{FinishReason, ModelEvent},
    store::{BlobId, Store},
};
use prost::Message;
use serde_json::{json, Value};

#[tokio::test]
async fn repeated_edits_and_recovered_turns_keep_the_original_file_snapshot() {
    let (directory, store) = fixtures::temp_store().await;
    let provider = fake_provider::FakeProvider::default();
    tool(
        &provider,
        "write",
        "Write",
        json!({"path":"/tmp/a", "contents":"first\n"}),
    );
    tool(
        &provider,
        "replace",
        "StrReplace",
        json!({"path":"/tmp/a", "old_string":"first", "new_string":"second"}),
    );
    done(&provider);
    let mut files = HashMap::from([("/tmp/a".into(), "original\n".into())]);
    let state = run(&store, &provider, "first", None, &mut files, false).await;
    assert_file(&store, &state, "/tmp/a", "original\n", "second\n").await;
    let edits = history_edits(&store, &state).await;
    assert_eq!(edits.len(), 2);
    assert_eq!(
        (edits[0].lines_added, edits[0].lines_removed),
        (Some(1), Some(1))
    );
    assert!(edits[0].diff_string.as_deref().unwrap().contains("+first"));
    assert!(edits[1].diff_string.as_deref().unwrap().contains("+second"));
    assert!(edits
        .iter()
        .all(|edit| edit.before_full_file_content.is_none()
            && edit.after_full_file_content.is_empty()));

    // Reopen the database and reconstruct the runtime from the wire checkpoint.
    let restored =
        pb::ConversationStateStructure::decode(state.encode_to_vec().as_slice()).unwrap();
    let reopened = Store::connect(&format!(
        "sqlite://{}",
        directory.path().join("test.db").display()
    ))
    .await
    .unwrap();
    let provider = fake_provider::FakeProvider::default();
    tool(
        &provider,
        "resume-write",
        "Write",
        json!({"path":"/tmp/a", "contents":"third\n"}),
    );
    done(&provider);
    let next = run(
        &reopened,
        &provider,
        "second",
        Some(restored),
        &mut files,
        false,
    )
    .await;
    assert_file(&reopened, &next, "/tmp/a", "original\n", "third\n").await;
    assert_eq!(
        next.file_states_v2["/tmp/a"].initial_content,
        state.file_states_v2["/tmp/a"].initial_content
    );
    assert!(next
        .root_prompt_messages_json
        .starts_with(&state.root_prompt_messages_json));
    assert_eq!(next.turns.len(), 2);
}

#[tokio::test]
async fn new_and_large_files_retain_full_snapshots_when_history_diffs_are_bounded() {
    let (_directory, store) = fixtures::temp_store().await;
    let provider = fake_provider::FakeProvider::default();
    let contents = "complete file content\n".repeat(5000);
    tool(
        &provider,
        "new-file",
        "Write",
        json!({"path":"/tmp/new", "contents":contents}),
    );
    done(&provider);
    let mut files = HashMap::new();
    let state = run(&store, &provider, "new-file", None, &mut files, false).await;
    assert_file(&store, &state, "/tmp/new", "", &contents).await;
    let edits = history_edits(&store, &state).await;
    assert_eq!(edits[0].lines_added, Some(5000));
    assert!(edits[0].diff_string.as_ref().unwrap().len() < contents.len());
    assert!(edits[0].after_full_file_content.is_empty());
}

#[tokio::test]
async fn rejected_writes_do_not_create_file_review_state() {
    let (_directory, store) = fixtures::temp_store().await;
    let provider = fake_provider::FakeProvider::default();
    tool(
        &provider,
        "rejected",
        "Write",
        json!({"path":"/tmp/a", "contents":"rejected\n"}),
    );
    done(&provider);
    let mut files = HashMap::from([("/tmp/a".into(), "original\n".into())]);
    let state = run(&store, &provider, "rejected", None, &mut files, true).await;
    assert!(state.file_states_v2.is_empty());
    assert!(history_edits(&store, &state).await.is_empty());
    assert_eq!(files["/tmp/a"], "original\n");
}

fn tool(provider: &fake_provider::FakeProvider, id: &str, name: &str, arguments: Value) {
    provider.push(vec![
        ModelEvent::Start {
            model_call_id: format!("model-{id}"),
        },
        ModelEvent::ToolCallStart {
            index: 0,
            call_id: id.into(),
            name: name.into(),
        },
        ModelEvent::ToolCallArgumentsDelta {
            index: 0,
            delta: arguments.to_string(),
        },
        ModelEvent::ToolCallEnd { index: 0 },
        ModelEvent::Done(FinishReason::ToolUse),
    ]);
}

fn done(provider: &fake_provider::FakeProvider) {
    provider.push(vec![
        ModelEvent::Start {
            model_call_id: "final".into(),
        },
        ModelEvent::TextStart,
        ModelEvent::TextDelta("done".into()),
        ModelEvent::TextEnd,
        ModelEvent::Done(FinishReason::Stop),
    ]);
}

async fn run(
    store: &Store,
    provider: &fake_provider::FakeProvider,
    request_id: &str,
    state: Option<pb::ConversationStateStructure>,
    files: &mut HashMap<String, String>,
    reject_writes: bool,
) -> pb::ConversationStateStructure {
    let mut available_blobs = state
        .iter()
        .flat_map(|state| state.file_states_v2.values())
        .flat_map(|file| [&file.initial_content, &file.content])
        .flatten()
        .cloned()
        .collect::<HashSet<_>>();
    let assets =
        PromptAssets::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("prompt/cursor"))
            .unwrap();
    let registry = TransportRegistry::new(
        store.clone(),
        Arc::new(provider.clone()),
        PromptCompiler::new(assets),
    );
    let handle = registry.get_or_create(request_id).await.unwrap();
    let mut output = handle.subscribe();
    let request = pb::AgentRunRequest {
        conversation_id: Some("file-review".into()),
        run_id: Some(request_id.into()),
        conversation_state: state,
        action: Some(pb::ConversationAction {
            action: Some(pb::conversation_action::Action::UserMessageAction(
                pb::UserMessageAction {
                    user_message: Some(pb::UserMessage {
                        text: "edit the file".into(),
                        message_id: format!("user-{request_id}"),
                        mode: pb::AgentMode::Agent as i32,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )),
            ..Default::default()
        }),
        requested_model: Some(pb::RequestedModel {
            model_id: "test-model".into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    handle
        .command(TransportCommand::Append {
            seqno: 0,
            message: Box::new(pb::AgentClientMessage {
                message: Some(pb::agent_client_message::Message::RunRequest(request)),
            }),
        })
        .await
        .unwrap();
    let mut seqno = 1;
    let mut final_state = None;
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(10), output.recv())
            .await
            .unwrap()
            .unwrap();
        let (flags, payload) = connect::decode_frames(&frame).unwrap().pop().unwrap();
        if flags & connect::END_STREAM_FLAG != 0 {
            assert!(
                !String::from_utf8_lossy(&payload).contains("error"),
                "unexpected stream error: {}",
                String::from_utf8_lossy(&payload)
            );
            break;
        }
        let server = pb::AgentServerMessage::decode(payload).unwrap();
        let message = match server.message {
            Some(pb::agent_server_message::Message::KvServerMessage(kv)) => {
                let Some(pb::kv_server_message::Message::SetBlobArgs(blob)) = kv.message else {
                    panic!("unexpected KV request");
                };
                assert_eq!(blob.blob_id, BlobId::digest(&blob.blob_data).as_bytes());
                available_blobs.insert(blob.blob_id);
                pb::agent_client_message::Message::KvClientMessage(pb::KvClientMessage {
                    id: kv.id,
                    message: Some(pb::kv_client_message::Message::SetBlobResult(
                        pb::SetBlobResult { error: None },
                    )),
                })
            }
            Some(pb::agent_server_message::Message::ExecServerMessage(exec)) => {
                let result = match exec.message.unwrap() {
                    pb::exec_server_message::Message::ReadArgs(args) => {
                        let result = match files.get(&args.path) {
                            Some(content) => pb::read_result::Result::Success(pb::ReadSuccess {
                                path: args.path,
                                output: Some(pb::read_success::Output::Content(content.clone())),
                                ..Default::default()
                            }),
                            None => pb::read_result::Result::FileNotFound(pb::ReadFileNotFound {
                                path: args.path,
                            }),
                        };
                        pb::exec_client_message::Message::ReadResult(pb::ReadResult {
                            result: Some(result),
                        })
                    }
                    pb::exec_server_message::Message::WriteArgs(args) => {
                        let result = if reject_writes {
                            pb::write_result::Result::Rejected(pb::WriteRejected {
                                path: args.path,
                                reason: "rejected by user".into(),
                            })
                        } else {
                            files.insert(args.path.clone(), args.file_text);
                            pb::write_result::Result::Success(pb::WriteSuccess {
                                path: args.path,
                                ..Default::default()
                            })
                        };
                        pb::exec_client_message::Message::WriteResult(pb::WriteResult {
                            result: Some(result),
                        })
                    }
                    other => panic!("unexpected exec: {other:?}"),
                };
                pb::agent_client_message::Message::ExecClientMessage(pb::ExecClientMessage {
                    id: exec.id,
                    message: Some(result),
                    ..Default::default()
                })
            }
            Some(pb::agent_server_message::Message::ConversationCheckpointUpdate(state)) => {
                // Every published snapshot must already exist, before final output.
                for file in state.file_states_v2.values() {
                    for id in [&file.initial_content, &file.content].into_iter().flatten() {
                        assert!(
                            available_blobs.contains(id),
                            "snapshot must reach Cursor before the checkpoint"
                        );
                        assert!(store
                            .get_blob(&BlobId::from_bytes(id).unwrap())
                            .await
                            .unwrap()
                            .is_some());
                    }
                }
                final_state = Some(state);
                continue;
            }
            _ => continue,
        };
        handle
            .command(TransportCommand::Append {
                seqno,
                message: Box::new(pb::AgentClientMessage {
                    message: Some(message),
                }),
            })
            .await
            .unwrap();
        seqno += 1;
    }
    final_state.expect("final checkpoint")
}

async fn assert_file(
    store: &Store,
    state: &pb::ConversationStateStructure,
    path: &str,
    before: &str,
    after: &str,
) {
    let file = state
        .file_states_v2
        .get(path)
        .expect("edited file must be included in review");
    for (id, expected) in [
        (file.initial_content.as_ref().unwrap(), before),
        (file.content.as_ref().unwrap(), after),
    ] {
        assert_eq!(
            store
                .get_blob(&BlobId::from_bytes(id).unwrap())
                .await
                .unwrap()
                .unwrap(),
            expected.as_bytes()
        );
    }
}

async fn history_edits(
    store: &Store,
    state: &pb::ConversationStateStructure,
) -> Vec<pb::EditSuccess> {
    let mut edits = Vec::new();
    for turn in &state.turns {
        let data = store
            .get_blob(&BlobId::from_bytes(turn).unwrap())
            .await
            .unwrap()
            .unwrap();
        let turn = pb::ConversationTurnStructure::decode(data.as_slice()).unwrap();
        let Some(pb::conversation_turn_structure::Turn::AgentConversationTurn(turn)) = turn.turn
        else {
            panic!("missing agent turn")
        };
        for step in turn.steps {
            let data = store
                .get_blob(&BlobId::from_bytes(&step).unwrap())
                .await
                .unwrap()
                .unwrap();
            let step = pb::ConversationStep::decode(data.as_slice()).unwrap();
            if let Some(pb::conversation_step::Message::ToolCall(call)) = step.message {
                if let Some(pb::tool_call::Tool::EditToolCall(edit)) = call.tool {
                    if let Some(pb::edit_result::Result::Success(success)) =
                        edit.result.and_then(|result| result.result)
                    {
                        edits.push(success);
                    }
                }
            }
        }
    }
    edits
}
