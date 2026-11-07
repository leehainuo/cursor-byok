//! Owns one client-loop inference stream per Cursor request ID.
use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use futures_util::StreamExt;
use parking_lot::Mutex;
use prost::Message;
use tokio::{
    sync::{mpsc, Mutex as AsyncMutex, Notify},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

use crate::{
    cursor::{
        protocol::{connect, proto::inference as pb},
        transport::{OutputHub, TransportRegistry},
    },
    model::{ModelInvocation, ModelType},
    provider::{ModelEvent, Provider},
    Error, Result,
};

use super::project;

#[derive(Clone)]
pub(crate) struct InferenceService {
    state: Arc<Mutex<HashMap<String, InferenceRoute>>>,
    changed: Arc<Notify>,
    registry: TransportRegistry,
}

#[derive(Clone)]
pub(crate) enum InferenceRoute {
    Local(Arc<Run>),
    Upstream,
}

pub(crate) struct Run {
    input: mpsc::Sender<pb::RunInferenceClientMessage>,
    output: Arc<OutputHub>,
    next_seqno: AsyncMutex<i64>,
}

impl InferenceService {
    pub fn new(registry: TransportRegistry) -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            changed: Arc::new(Notify::new()),
            registry,
        }
    }

    pub fn route(&self, request_id: &str) -> Option<InferenceRoute> {
        self.state.lock().get(request_id).cloned()
    }

    pub async fn wait_route(&self, request_id: &str) -> InferenceRoute {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if let Some(route) = self.route(request_id) {
                return route;
            }
            changed.await;
        }
    }

    pub async fn begin(
        &self,
        request_id: &str,
        message: &pb::RunInferenceClientMessage,
    ) -> Result<InferenceRoute> {
        let Some(pb::run_inference_client_message::Message::RunRequest(request)) =
            message.message.as_ref()
        else {
            return Err(Error::Protocol(
                "first RunInference message must start a run".into(),
            ));
        };
        let model_id = request
            .requested_model
            .as_ref()
            .map(|model| model.model_id.as_str())
            .filter(|model| !model.is_empty())
            .ok_or_else(|| Error::Protocol("RunInference model is required".into()))?;
        if request.conversation_id.is_empty() {
            return Err(Error::Protocol(
                "RunInference conversation ID is required".into(),
            ));
        }
        let configured = self.registry.store().model(model_id).await?;
        let local = configured.is_some() || model_id.starts_with(crate::plugin::ADAPTER_ID_PREFIX);
        let route = if local {
            tracing::info!(
                request_id,
                model_id,
                "routing Cursor RunInference to BYOK provider"
            );
            let (sender, receiver) = mpsc::channel(64);
            let output = Arc::new(OutputHub::default());
            let run = Arc::new(Run {
                input: sender,
                output: output.clone(),
                next_seqno: AsyncMutex::new(0),
            });
            tokio::spawn(serve(
                receiver,
                output,
                self.registry.provider(),
                request_id.into(),
                request.conversation_id.clone(),
                model_id.into(),
                configured.map(|model| (model.display_name, model.model_type)),
            ));
            InferenceRoute::Local(run)
        } else {
            tracing::info!(
                request_id,
                model_id,
                "routing Cursor RunInference to Cursor upstream"
            );
            InferenceRoute::Upstream
        };
        let mut state = self.state.lock();
        let route = state
            .entry(request_id.into())
            .or_insert_with(|| route.clone())
            .clone();
        drop(state);
        self.changed.notify_waiters();
        Ok(route)
    }

    pub fn finish(&self, request_id: &str) {
        self.state.lock().remove(request_id);
    }
}

impl Run {
    pub async fn append(&self, seqno: i64, message: pb::RunInferenceClientMessage) -> Result<()> {
        let mut next = self.next_seqno.lock().await;
        if seqno < *next {
            return Ok(());
        }
        if seqno != *next {
            return Err(Error::Protocol(format!(
                "RunInference append sequence {seqno} differs from expected {}",
                *next
            )));
        }
        self.input
            .send(message)
            .await
            .map_err(|_| Error::Protocol("RunInference stream has ended".into()))?;
        *next += 1;
        Ok(())
    }

    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<Bytes> {
        self.output.subscribe()
    }
}

async fn serve(
    mut input: mpsc::Receiver<pb::RunInferenceClientMessage>,
    output: Arc<OutputHub>,
    provider: Arc<dyn Provider>,
    run_id: String,
    conversation_id: String,
    model_id: String,
    model_info: Option<(String, ModelType)>,
) {
    let invocations = Arc::new(Mutex::new(HashMap::<String, CancellationToken>::new()));
    let mut tasks = JoinSet::new();
    let mut finished = false;
    let mut provider_call_index = 0;
    while let Some(message) = input.recv().await {
        use pb::run_inference_client_message::Message;
        match message.message {
            Some(Message::RunRequest(request)) => {
                let ready = run_ready(request.requested_model, model_info.as_ref());
                emit(
                    &output,
                    pb::run_inference_server_message::Message::RunReady(ready),
                );
            }
            Some(Message::InvokeModel(invoke)) => {
                let id = invoke.invocation_id.clone();
                let Some(request) = invoke.request else {
                    invocation_error(&output, id, "RunInference model request is missing");
                    continue;
                };
                if invoke.delta_base.is_some() || invoke.compressed_request.is_some() {
                    invocation_error(
                        &output,
                        id,
                        "RunInference delta or compressed requests are not supported",
                    );
                    continue;
                }
                let request = match project(&request, &model_id) {
                    Ok(request) => request,
                    Err(error) => {
                        invocation_error(&output, id, &error.to_string());
                        continue;
                    }
                };
                let cancellation = CancellationToken::new();
                invocations.lock().insert(id.clone(), cancellation.clone());
                tasks.spawn(serve_invocation(
                    provider.clone(),
                    output.clone(),
                    invocations.clone(),
                    cancellation,
                    ModelInvocation {
                        call_id: id,
                        run_id: run_id.clone(),
                        conversation_id: conversation_id.clone(),
                        provider_call_index,
                        request,
                    },
                ));
                provider_call_index += 1;
            }
            Some(Message::CancelInvocation(cancel)) => {
                if let Some(token) = invocations.lock().remove(&cancel.invocation_id) {
                    token.cancel();
                }
            }
            Some(Message::FinishRun(finish)) => {
                finished = true;
                if finish.aborted {
                    for (_, token) in invocations.lock().drain() {
                        token.cancel();
                    }
                }
                break;
            }
            Some(
                Message::ClientHeartbeat(_)
                | Message::AgentEvent(_)
                | Message::RunRequestLog(_)
                | Message::ResolveSubagentModels(_)
                | Message::ClientFirstCallTimings(_),
            )
            | None => {}
        }
    }
    if !finished {
        for (_, token) in invocations.lock().drain() {
            token.cancel();
        }
    }
    while tasks.join_next().await.is_some() {}
    output.emit(connect::encode_end_stream());
    output.close();
}

fn run_ready(
    requested_model: Option<pb::InferenceRequestedModel>,
    model_info: Option<&(String, ModelType)>,
) -> pb::RunInferenceRunReady {
    let (display_name, model_type) = model_info
        .map(|(name, kind)| (name.clone(), *kind))
        .unwrap_or_else(|| (String::new(), ModelType::OpenAi));
    let anthropic = model_type == ModelType::Anthropic;
    let name = display_name.to_ascii_lowercase();
    let claude = anthropic && name.contains("claude");
    let gpt5 = !anthropic && name.starts_with("gpt-5");
    pb::RunInferenceRunReady {
        resolved_model: requested_model,
        routed_model_display_name: (!display_name.is_empty()).then_some(display_name),
        prompt_model_metadata: Some(pb::RunInferencePromptModelMetadata {
            vendor: if anthropic { "anthropic" } else { "openai" }.into(),
            prompt_version: "latest".into(),
            is_claude_4x: claude,
            is_sonnet4: claude && name.contains("sonnet"),
            is_gpt5_family: gpt5,
            is_gpt5: gpt5,
            persona: "auto".into(),
            self_identity_name: "Cursor".into(),
            agent_mode_configs: agent_mode_configs(),
            ask_question_config: Some(pb::RunInferenceAskQuestionConfig {
                tool_name: "AskQuestion".into(),
                ..Default::default()
            }),
            ..Default::default()
        }),
        accepts_aborted_finish: true,
        ..Default::default()
    }
}

async fn serve_invocation(
    provider: Arc<dyn Provider>,
    output: Arc<OutputHub>,
    invocations: Arc<Mutex<HashMap<String, CancellationToken>>>,
    cancellation: CancellationToken,
    invocation: ModelInvocation,
) {
    let id = invocation.call_id.clone();
    let mut stream = provider.stream(invocation, cancellation);
    let mut tool_calls = HashMap::<usize, (String, String, String)>::new();
    let mut error = None;
    while let Some(next) = stream.next().await {
        match next {
            Ok(event) => {
                if let Some(response) = model_response(event, &mut tool_calls) {
                    emit(
                        &output,
                        pb::run_inference_server_message::Message::InvocationResponse(
                            pb::RunInferenceInvocationResponse {
                                invocation_id: id.clone(),
                                response: Some(response),
                            },
                        ),
                    );
                }
            }
            Err(value) => {
                error = Some(value.to_string());
                break;
            }
        }
    }
    invocations.lock().remove(&id);
    let message = pb::RunInferenceInvocationEnd {
        invocation_id: id,
        error: error.map(|message| pb::RunInferenceInvocationError {
            code: 13,
            message,
            ..Default::default()
        }),
        ..Default::default()
    };
    emit(
        &output,
        pb::run_inference_server_message::Message::InvocationEnd(message),
    );
}

fn model_response(
    event: ModelEvent,
    tool_calls: &mut HashMap<usize, (String, String, String)>,
) -> Option<pb::InferenceStreamResponse> {
    use pb::inference_stream_response::Response;
    let response = match event {
        ModelEvent::TextDelta(text) => Response::TextPart(pb::InferenceTextStreamPart {
            text,
            is_final: false,
        }),
        ModelEvent::TextEnd => Response::TextPart(pb::InferenceTextStreamPart {
            text: String::new(),
            is_final: true,
        }),
        ModelEvent::ThinkingDelta(text) => {
            Response::ThinkingPart(pb::InferenceThinkingStreamPart {
                text,
                is_final: false,
                ..Default::default()
            })
        }
        ModelEvent::ThinkingEnd => Response::ThinkingPart(pb::InferenceThinkingStreamPart {
            text: String::new(),
            is_final: true,
            ..Default::default()
        }),
        ModelEvent::ThinkingSignature(signature) => {
            Response::ThinkingPart(pb::InferenceThinkingStreamPart {
                text: String::new(),
                signature: Some(signature),
                is_final: true,
                ..Default::default()
            })
        }
        ModelEvent::RedactedThinking(data) => {
            Response::ThinkingPart(pb::InferenceThinkingStreamPart {
                text: String::new(),
                redacted_data: Some(data),
                is_final: true,
                ..Default::default()
            })
        }
        ModelEvent::ToolCallStart {
            index,
            call_id,
            name,
        } => {
            tool_calls.insert(index, (call_id.clone(), name.clone(), String::new()));
            Response::ToolCallPart(pb::InferenceToolCallStreamPart {
                tool_call_id: call_id,
                tool_name: name,
                args: String::new(),
                is_complete: false,
                tool_index: i32::try_from(index).ok(),
                ..Default::default()
            })
        }
        ModelEvent::ToolCallArgumentsDelta { index, delta } => {
            let (call_id, _, arguments) = tool_calls.get_mut(&index)?;
            arguments.push_str(&delta);
            Response::ToolCallPart(pb::InferenceToolCallStreamPart {
                tool_call_id: call_id.clone(),
                tool_name: String::new(),
                args: delta,
                is_complete: false,
                tool_index: i32::try_from(index).ok(),
                ..Default::default()
            })
        }
        ModelEvent::ToolCallEnd { index } => {
            let (call_id, name, arguments) = tool_calls.remove(&index)?;
            Response::ToolCallPart(pb::InferenceToolCallStreamPart {
                tool_call_id: call_id,
                tool_name: name,
                args: arguments,
                is_complete: true,
                tool_index: i32::try_from(index).ok(),
                ..Default::default()
            })
        }
        ModelEvent::Usage(usage) => Response::ExtendedUsage(pb::InferenceExtendedUsageInfo {
            input_tokens: count(usage.input_tokens),
            output_tokens: count(usage.output_tokens),
            cache_read_tokens: count(usage.cache_read_tokens),
            cache_write_tokens: count(usage.cache_write_tokens),
            ..Default::default()
        }),
        ModelEvent::Start { model_call_id } => {
            Response::InvocationId(pb::InferenceInvocationIdInfo {
                invocation_id: model_call_id,
            })
        }
        ModelEvent::TextStart
        | ModelEvent::ThinkingStart
        | ModelEvent::ProviderReplayState(_)
        | ModelEvent::Done(_) => return None,
    };
    Some(pb::InferenceStreamResponse {
        response: Some(response),
        ..Default::default()
    })
}

fn agent_mode_configs() -> Vec<pb::RunInferenceAgentModeConfig> {
    const MODES: [&str; 6] = ["agent", "chat", "plan", "debug", "triage", "multitask"];
    const SWITCH: [&str; 5] = ["agent", "chat", "plan", "debug", "multitask"];
    MODES
        .into_iter()
        .map(|mode| pb::RunInferenceAgentModeConfig {
            mode_id: mode.into(),
            enable_ask_question_tool: true,
            switch_mode_tool_config: (mode == "agent").then(|| {
                pb::RunInferenceSwitchModeToolConfig {
                    target_modes: SWITCH.into_iter().map(str::to_owned).collect(),
                }
            }),
        })
        .collect()
}

fn count(value: Option<u64>) -> i32 {
    value.unwrap_or_default().min(i32::MAX as u64) as i32
}

fn invocation_error(output: &OutputHub, id: String, message: &str) {
    emit(
        output,
        pb::run_inference_server_message::Message::InvocationEnd(pb::RunInferenceInvocationEnd {
            invocation_id: id,
            error: Some(pb::RunInferenceInvocationError {
                code: 3,
                message: message.into(),
                ..Default::default()
            }),
            ..Default::default()
        }),
    );
}

fn emit(output: &OutputHub, message: pb::run_inference_server_message::Message) {
    let message = pb::RunInferenceServerMessage {
        message: Some(message),
    };
    if let Ok(frame) = connect::encode_message(&message) {
        output.emit(frame);
    }
}

pub(crate) fn decode_append(data: &[u8]) -> Result<pb::RunInferenceClientMessage> {
    pb::RunInferenceClientMessage::decode(data).map_err(Error::from)
}
