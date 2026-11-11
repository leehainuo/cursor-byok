//! Exercises the client-owned Agent inference handshake and model stream.
use std::sync::{Arc, Mutex};

use super::{router_with_proxy, CursorProxy};
use crate::{
    cursor::{
        prompting::{PromptAssets, PromptCompiler},
        protocol::{
            connect,
            proto::{agent::v1 as agent, aiserver::v1 as ai, inference as pb},
        },
        services::knowledge::KnowledgeService,
        transport::TransportRegistry,
    },
    model::{
        ModelConfigInput, ModelInvocation, ModelRequest, ModelType, ProjectedContent, Usage,
        OPENAI_CHAT_ENDPOINT,
    },
    network::NetworkClients,
    provider::{FinishReason, ModelEvent, Provider, ProviderStream},
    store::Store,
};
use axum::{
    body::{Body, Bytes},
    http::{header, Request, StatusCode},
};
use futures_util::{stream, StreamExt};
use prost::Message;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

#[tokio::test]
async fn client_loop_receives_ready_text_tool_call_and_completion() {
    let (_directory, store) = temp_store().await;
    let model = store.create_model(&model_input()).await.unwrap();
    let provider = FakeProvider::default();
    provider.push(vec![
        ModelEvent::Start {
            model_call_id: "invoke-1".into(),
        },
        ModelEvent::ThinkingStart,
        ModelEvent::ThinkingDelta("Checking the file".into()),
        ModelEvent::ThinkingEnd,
        ModelEvent::ThinkingSignature("signature-1".into()),
        ModelEvent::TextStart,
        ModelEvent::TextDelta("Let me check".into()),
        ModelEvent::TextEnd,
        ModelEvent::ToolCallStart {
            index: 0,
            call_id: "call-1".into(),
            name: "Read".into(),
        },
        ModelEvent::ToolCallArgumentsDelta {
            index: 0,
            delta: "{\"path\":\"a.rs\"}".into(),
        },
        ModelEvent::ToolCallEnd { index: 0 },
        ModelEvent::Usage(Usage {
            input_tokens: Some(80),
            context_input_tokens: Some(120),
            output_tokens: Some(40),
            total_tokens: Some(160),
            cache_read_tokens: Some(20),
            cache_write_tokens: Some(10),
            reasoning_tokens: Some(8),
        }),
        ModelEvent::Done(FinishReason::ToolUse),
    ]);
    let (_router_directory, router) = router(store, provider.clone());
    let request_id = "inference-test";
    append(
        &router,
        request_id,
        0,
        pb::RunInferenceClientMessage {
            message: Some(pb::run_inference_client_message::Message::RunRequest(
                pb::RunInferenceRunRequest {
                    conversation_id: "conversation-test".into(),
                    requested_model: Some(pb::InferenceRequestedModel {
                        model_id: model.model_hash.clone(),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )),
        },
    )
    .await;
    let response = router
        .clone()
        .oneshot(
            Request::post("/aiserver.v1.InferenceService/RunInferenceSSE")
                .header(header::CONTENT_TYPE, "application/proto")
                .body(Body::from(
                    connect::encode_message(&agent::BidiRequestId {
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
    let ready = next_message(&mut stream).await;
    let Some(pb::run_inference_server_message::Message::RunReady(ready)) = ready.message else {
        panic!("expected run_ready");
    };
    assert_eq!(ready.resolved_model.unwrap().model_id, model.model_hash);
    assert!(!ready.prompt_model_metadata.unwrap().is_gpt5);

    append(
        &router,
        request_id,
        1,
        pb::RunInferenceClientMessage {
            message: Some(pb::run_inference_client_message::Message::InvokeModel(
                pb::RunInferenceInvokeModel {
                    invocation_id: "invoke-1".into(),
                    request: Some(pb::InferenceStreamRequest {
                        messages: vec![pb::InferenceCoreMessage {
                            role: 1,
                            content: Some(pb::inference_core_message::Content::Text(
                                "Read a.rs".into(),
                            )),
                            ..Default::default()
                        }],
                        tools: vec![pb::InferenceAgentTool {
                            name: "Read".into(),
                            description: "Read a file".into(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )),
        },
    )
    .await;

    let mut text = String::new();
    let mut tool = String::new();
    let mut completed_tool = None;
    let mut signature = None;
    let mut invocation_id = None;
    let mut usage = None;
    let mut extended_usage = None;
    loop {
        let message = next_message(&mut stream).await;
        match message.message.unwrap() {
            pb::run_inference_server_message::Message::InvocationResponse(response) => {
                match response.response.unwrap().response.unwrap() {
                    pb::inference_stream_response::Response::TextPart(part) => {
                        text.push_str(&part.text)
                    }
                    pb::inference_stream_response::Response::ToolCallPart(part) => {
                        if part.is_complete {
                            completed_tool = Some(part.args);
                        } else if part.tool_name.is_empty() {
                            tool.push_str(&part.args);
                        }
                    }
                    pb::inference_stream_response::Response::ThinkingPart(part) => {
                        signature = signature.or(part.signature);
                    }
                    pb::inference_stream_response::Response::InvocationId(info) => {
                        invocation_id = Some(info.invocation_id);
                    }
                    pb::inference_stream_response::Response::Usage(value) => {
                        usage = Some(value);
                    }
                    pb::inference_stream_response::Response::ExtendedUsage(value) => {
                        extended_usage = Some(value);
                    }
                    _ => {}
                }
            }
            pb::run_inference_server_message::Message::InvocationEnd(end) => {
                assert_eq!(end.invocation_id, "invoke-1");
                assert!(end.error.is_none());
                break;
            }
            _ => {}
        }
    }
    assert_eq!(text, "Let me check");
    assert_eq!(tool, "{\"path\":\"a.rs\"}");
    assert_eq!(completed_tool.as_deref(), Some(tool.as_str()));
    assert_eq!(signature.as_deref(), Some("signature-1"));
    assert_eq!(invocation_id.as_deref(), Some("invoke-1"));
    let usage = usage.expect("usage");
    assert_eq!(usage.prompt_tokens, 120);
    assert_eq!(usage.completion_tokens, 40);
    assert_eq!(usage.total_tokens, Some(160));
    let extended = extended_usage.expect("extended usage");
    assert_eq!(extended.input_tokens, 80);
    assert_eq!(extended.output_tokens, 40);
    assert_eq!(extended.cache_read_tokens, 20);
    assert_eq!(extended.cache_write_tokens, 10);
    assert_eq!(extended.max_tokens, 128_000);
    assert_eq!(extended.reasoning_tokens, Some(8));
    let requests = provider.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].history.len(), 1);
    assert_eq!(requests[0].prompt.tools[0].name, "Read");
    assert!(matches!(
        &requests[0].history[0].content,
        ProjectedContent::Parts(_)
    ));

    append(
        &router,
        request_id,
        2,
        pb::RunInferenceClientMessage {
            message: Some(pb::run_inference_client_message::Message::FinishRun(
                pb::RunInferenceFinishRun { aborted: false },
            )),
        },
    )
    .await;
    let end = tokio::time::timeout(std::time::Duration::from_secs(3), stream.next())
        .await
        .expect("end frame timed out")
        .expect("inference stream ended before its end frame")
        .unwrap();
    assert_eq!(
        connect::decode_frames(&end).unwrap()[0].0,
        connect::END_STREAM_FLAG
    );
}

#[tokio::test]
async fn client_loop_uses_run_request_selector_window_when_invoke_omits_context() {
    let (_directory, store) = temp_store().await;
    let provider = FakeProvider::default();
    provider.push(vec![
        ModelEvent::Start {
            model_call_id: "invoke-1".into(),
        },
        ModelEvent::Usage(Usage {
            input_tokens: Some(80),
            context_input_tokens: Some(120),
            output_tokens: Some(40),
            total_tokens: Some(160),
            cache_read_tokens: Some(20),
            cache_write_tokens: Some(10),
            reasoning_tokens: Some(8),
        }),
        ModelEvent::Done(FinishReason::Stop),
    ]);
    let (_router_directory, router) = router(store, provider.clone());
    let request_id = "plugin-inference-window";
    let model_id = "plugin:dev.cursorbyok.plugins.magpie/magpie/test/grok-4.6";
    append(
        &router,
        request_id,
        0,
        pb::RunInferenceClientMessage {
            message: Some(pb::run_inference_client_message::Message::RunRequest(
                pb::RunInferenceRunRequest {
                    conversation_id: "conversation-plugin".into(),
                    requested_model: Some(pb::InferenceRequestedModel {
                        model_id: model_id.into(),
                        parameters: vec![pb::InferenceModelParameterValue {
                            id: "context".into(),
                            value: "1m".into(),
                        }],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )),
        },
    )
    .await;
    let response = router
        .clone()
        .oneshot(
            Request::post("/aiserver.v1.InferenceService/RunInferenceSSE")
                .header(header::CONTENT_TYPE, "application/proto")
                .body(Body::from(
                    connect::encode_message(&agent::BidiRequestId {
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
    let ready = next_message(&mut stream).await;
    assert!(matches!(
        ready.message,
        Some(pb::run_inference_server_message::Message::RunReady(_))
    ));

    append(
        &router,
        request_id,
        1,
        pb::RunInferenceClientMessage {
            message: Some(pb::run_inference_client_message::Message::InvokeModel(
                pb::RunInferenceInvokeModel {
                    invocation_id: "invoke-1".into(),
                    request: Some(pb::InferenceStreamRequest {
                        messages: vec![pb::InferenceCoreMessage {
                            role: 1,
                            content: Some(pb::inference_core_message::Content::Text(
                                "Read a.rs".into(),
                            )),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )),
        },
    )
    .await;

    let mut extended_usage = None;
    loop {
        let message = next_message(&mut stream).await;
        match message.message.unwrap() {
            pb::run_inference_server_message::Message::InvocationResponse(response) => {
                if let pb::inference_stream_response::Response::ExtendedUsage(value) =
                    response.response.unwrap().response.unwrap()
                {
                    extended_usage = Some(value);
                }
            }
            pb::run_inference_server_message::Message::InvocationEnd(end) => {
                assert_eq!(end.invocation_id, "invoke-1");
                assert!(end.error.is_none());
                break;
            }
            _ => {}
        }
    }
    let extended = extended_usage.expect("extended usage");
    assert_eq!(extended.max_tokens, 1_000_000);
    let requests = provider.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].model.context_window_tokens, Some(1_000_000));
}

async fn append(
    router: &axum::Router,
    request_id: &str,
    seqno: i64,
    message: pb::RunInferenceClientMessage,
) {
    let request = ai::BidiAppendRequest {
        request_id: Some(ai::BidiRequestId {
            request_id: request_id.into(),
        }),
        data: hex::encode(message.encode_to_vec()),
        append_seqno: seqno,
        ..Default::default()
    };
    let response = router
        .clone()
        .oneshot(
            Request::post("/aiserver.v1.BidiService/BidiAppend")
                .header(header::CONTENT_TYPE, "application/proto")
                .body(Body::from(request.encode_to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn next_message<S>(stream: &mut S) -> pb::RunInferenceServerMessage
where
    S: futures_util::Stream<Item = Result<Bytes, axum::Error>> + Unpin,
{
    let frame = tokio::time::timeout(std::time::Duration::from_secs(3), stream.next())
        .await
        .expect("inference frame timed out")
        .expect("inference stream ended")
        .unwrap();
    let (flags, bytes) = connect::decode_frames(&frame).unwrap().pop().unwrap();
    assert_eq!(flags, 0);
    pb::RunInferenceServerMessage::decode(bytes).unwrap()
}

fn router(store: crate::store::Store, provider: FakeProvider) -> (tempfile::TempDir, axum::Router) {
    let directory = tempfile::tempdir().unwrap();
    let assets = PromptAssets::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("prompt/cursor")
            .as_path(),
    )
    .unwrap();
    let clients = NetworkClients::new(store.clone());
    let registry = TransportRegistry::new(store, Arc::new(provider), PromptCompiler::new(assets));
    let proxy = CursorProxy::cursor(clients);
    let knowledge = KnowledgeService::with_root(directory.path().join("rules")).unwrap();
    (directory, router_with_proxy(registry, proxy, knowledge))
}

async fn temp_store() -> (tempfile::TempDir, Store) {
    let directory = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", directory.path().join("test.db").display());
    let store = Store::connect(&url).await.unwrap();
    (directory, store)
}

#[derive(Clone, Default)]
struct FakeProvider {
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    events: Arc<Mutex<Vec<ModelEvent>>>,
}

impl FakeProvider {
    fn push(&self, events: Vec<ModelEvent>) {
        *self.events.lock().unwrap() = events;
    }

    fn requests(&self) -> Vec<ModelRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Provider for FakeProvider {
    fn stream(
        &self,
        invocation: ModelInvocation,
        _cancellation: CancellationToken,
    ) -> ProviderStream {
        self.requests.lock().unwrap().push(invocation.request);
        let events = std::mem::take(&mut *self.events.lock().unwrap());
        Box::pin(stream::iter(events.into_iter().map(Ok)))
    }
}

fn model_input() -> ModelConfigInput {
    ModelConfigInput {
        sort_order: 1,
        display_name: "Test model".into(),
        group_name: None,
        model_type: ModelType::OpenAi,
        base_url: "https://example.com/v1".into(),
        use_full_url: false,
        api_key: "test-key".into(),
        tooltip_data: "Test model".into(),
        model_id: "test-model".into(),
        reasoning_effort: None,
        openai_endpoint: OPENAI_CHAT_ENDPOINT.into(),
        openai_extra_params_enabled: false,
        openai_extra_params: serde_json::json!({}),
        custom_headers_enabled: false,
        custom_headers: serde_json::json!({}),
        anthropic_extra_params_enabled: false,
        anthropic_extra_params: serde_json::json!({}),
        context_window_tokens: Some(128_000),
        max_completion_tokens: None,
        anthropic_max_tokens: None,
        anthropic_thinking_effort: None,
        thinking_budget_tokens: None,
    }
}
