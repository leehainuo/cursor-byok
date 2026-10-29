use std::{
    collections::{BTreeMap, HashSet},
    convert::Infallible,
};

use axum::{
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::{
    model::Usage,
    provider::{FinishReason, ModelEvent, ProviderStream},
    Error, Result,
};

use super::protocol::Protocol;

#[derive(Default)]
struct ToolOutput {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct Output {
    text: String,
    thinking: String,
    tools: BTreeMap<usize, ToolOutput>,
    usage: Option<Usage>,
    finish: Option<FinishReason>,
}

impl Output {
    fn update(&mut self, event: &ModelEvent) {
        match event {
            ModelEvent::TextDelta(delta) => self.text.push_str(delta),
            ModelEvent::ThinkingDelta(delta) => self.thinking.push_str(delta),
            ModelEvent::ToolCallStart {
                index,
                call_id,
                name,
            } => {
                self.tools.insert(
                    *index,
                    ToolOutput {
                        id: call_id.clone(),
                        name: name.clone(),
                        arguments: String::new(),
                    },
                );
            }
            ModelEvent::ToolCallArgumentsDelta { index, delta } => {
                if let Some(tool) = self.tools.get_mut(index) {
                    tool.arguments.push_str(delta);
                }
            }
            ModelEvent::Usage(usage) => self.usage = Some(*usage),
            ModelEvent::Done(reason) => self.finish = Some(*reason),
            _ => {}
        }
    }

    fn json(&self, protocol: Protocol, id: &str, model: &str) -> Value {
        let usage = self.usage_json(protocol);
        let finish = match self.finish.unwrap_or(FinishReason::Stop) {
            FinishReason::Stop => "stop",
            FinishReason::Length => "length",
            FinishReason::ToolUse => "tool_calls",
        };
        match protocol {
            Protocol::Chat => json!({"id":id,"object":"chat.completion","created":0,"model":model,
                "choices":[{"index":0,"message":{"role":"assistant","content":self.text,
                    "tool_calls":self.tools.values().map(|tool| json!({"id":tool.id,"type":"function",
                        "function":{"name":tool.name,"arguments":tool.arguments}})).collect::<Vec<_>>()},"finish_reason":finish}],
                "usage":usage}),
            Protocol::Responses => {
                let mut items = Vec::new();
                if !self.text.is_empty() {
                    items.push(json!({"id":format!("msg_{id}"),"type":"message","status":"completed",
                    "role":"assistant","content":[{"type":"output_text","text":self.text,"annotations":[]}]}));
                }
                items.extend(self.tools.values().map(|tool| json!({"type":"function_call","id":format!("fc_{}",tool.id),
                    "call_id":tool.id,"name":tool.name,"arguments":tool.arguments,"status":"completed"})));
                json!({"id":id,"object":"response","created_at":0,"status":"completed","model":model,
                    "output":items,"output_text":self.text,
                    "usage":usage})
            }
            Protocol::Messages => {
                let mut content = Vec::new();
                if !self.text.is_empty() {
                    content.push(json!({"type":"text","text":self.text}));
                }
                content.extend(self.tools.values().map(|tool| {
                    json!({"type":"tool_use","id":tool.id,"name":tool.name,
                    "input":serde_json::from_str::<Value>(&tool.arguments).unwrap_or(json!({}))})
                }));
                json!({"id":id,"type":"message","role":"assistant","model":model,"content":content,
                    "stop_reason":if self.tools.is_empty() { match self.finish { Some(FinishReason::Length) => "max_tokens", _ => "end_turn" } } else { "tool_use" },
                    "stop_sequence":null,"usage":usage})
            }
        }
    }

    fn usage_json(&self, protocol: Protocol) -> Value {
        let usage = self.usage.unwrap_or_default();
        let input = usage
            .context_input_tokens
            .or(usage.input_tokens)
            .unwrap_or(0);
        let output = usage.output_tokens.unwrap_or(0);
        match protocol {
            Protocol::Chat => {
                let mut value = json!({"prompt_tokens":input,"completion_tokens":output,
                    "total_tokens":usage.total_tokens.unwrap_or(input.saturating_add(output))});
                if let Some(cached) = usage.cache_read_tokens {
                    value["prompt_tokens_details"] = json!({"cached_tokens":cached});
                }
                if let Some(reasoning) = usage.reasoning_tokens {
                    value["completion_tokens_details"] = json!({"reasoning_tokens":reasoning});
                }
                value
            }
            Protocol::Responses => {
                let mut value = json!({"input_tokens":input,"output_tokens":output,
                    "total_tokens":usage.total_tokens.unwrap_or(input.saturating_add(output))});
                if let Some(cached) = usage.cache_read_tokens {
                    value["input_tokens_details"] = json!({"cached_tokens":cached});
                }
                if let Some(reasoning) = usage.reasoning_tokens {
                    value["output_tokens_details"] = json!({"reasoning_tokens":reasoning});
                }
                value
            }
            Protocol::Messages => {
                let uncached =
                    usage
                        .context_input_tokens
                        .map_or(usage.input_tokens.unwrap_or(0), |total| {
                            total
                                .saturating_sub(usage.cache_read_tokens.unwrap_or(0))
                                .saturating_sub(usage.cache_write_tokens.unwrap_or(0))
                        });
                let mut value = json!({"input_tokens":uncached,"output_tokens":output});
                if let Some(cached) = usage.cache_read_tokens {
                    value["cache_read_input_tokens"] = json!(cached);
                }
                if let Some(written) = usage.cache_write_tokens {
                    value["cache_creation_input_tokens"] = json!(written);
                }
                value
            }
        }
    }
}

pub(super) async fn complete(
    protocol: Protocol,
    mut stream: ProviderStream,
    id: &str,
    model: &str,
) -> Result<Json<Value>> {
    let mut output = Output::default();
    while let Some(event) = stream.next().await {
        output.update(&event?);
    }
    if output.finish.is_none() {
        return Err(Error::Provider(
            "model stream ended without completion".into(),
        ));
    }
    Ok(Json(output.json(protocol, id, model)))
}

struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

pub(super) fn streamed(
    protocol: Protocol,
    mut provider: ProviderStream,
    cancellation: CancellationToken,
    id: String,
    model: String,
) -> Response {
    let events = async_stream::stream! {
        let _cancel = CancelOnDrop(cancellation);
        let mut output = Output::default();
        if matches!(protocol, Protocol::Chat) {
            yield Ok::<Event, Infallible>(Event::default().data(chat_chunk(&id, &model, json!({"role":"assistant"}), Value::Null).to_string()));
        } else if matches!(protocol, Protocol::Responses) {
            yield Ok(Event::default().event("response.created").data(json!({"type":"response.created","response":{"id":id,"status":"in_progress","model":model}}).to_string()));
            yield Ok(Event::default().event("response.in_progress").data(json!({"type":"response.in_progress","response":{"id":id,"status":"in_progress","model":model}}).to_string()));
        } else {
            yield Ok(Event::default().event("message_start").data(json!({"type":"message_start","message":{"id":id,"type":"message","role":"assistant","model":model,"content":[],"usage":{"input_tokens":0,"output_tokens":0}}}).to_string()));
        }
        let mut text_started = false;
        let mut text_closed = false;
        let mut closed_tools = HashSet::new();
        while let Some(result) = provider.next().await {
            match result {
                Ok(event) => {
                    if matches!(event, ModelEvent::TextDelta(_)) && !text_started && !matches!(protocol, Protocol::Chat) {
                        for (name, value) in stream_events(protocol, &ModelEvent::TextStart, &id, &model, &output, &mut text_started) {
                            yield Ok(Event::default().event(name).data(value.to_string()));
                        }
                    }
                    if matches!(event, ModelEvent::Done(_)) {
                        if text_started && !text_closed {
                            for (name, value) in stream_events(protocol, &ModelEvent::TextEnd, &id, &model, &output, &mut text_started) {
                                yield Ok(Event::default().event(name).data(value.to_string()));
                            }
                        }
                        for index in output.tools.keys().filter(|index| !closed_tools.contains(*index)).copied().collect::<Vec<_>>() {
                            for (name, value) in stream_events(protocol, &ModelEvent::ToolCallEnd { index }, &id, &model, &output, &mut text_started) {
                                yield Ok(Event::default().event(name).data(value.to_string()));
                            }
                        }
                    }
                    output.update(&event);
                    for (name, value) in stream_events(protocol, &event, &id, &model, &output, &mut text_started) {
                        yield Ok(Event::default().event(name).data(value.to_string()));
                    }
                    if matches!(event, ModelEvent::TextEnd | ModelEvent::Done(_)) {
                        text_closed = true;
                    }
                    if let ModelEvent::ToolCallEnd { index } = event {
                        closed_tools.insert(index);
                    }
                }
                Err(error) => {
                    yield Ok(Event::default().event("error").data(json!({"error":{"message":error.to_string(),"type":"upstream_error"}}).to_string()));
                    return;
                }
            }
        }
        if output.finish.is_some() && matches!(protocol, Protocol::Chat) {
            yield Ok(Event::default().data("[DONE]"));
        }
    };
    Sse::new(events)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn chat_chunk(id: &str, model: &str, delta: Value, finish: Value) -> Value {
    json!({"id":id,"object":"chat.completion.chunk","created":0,"model":model,
        "choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
}

fn stream_events(
    protocol: Protocol,
    event: &ModelEvent,
    id: &str,
    model: &str,
    output: &Output,
    text_started: &mut bool,
) -> Vec<(&'static str, Value)> {
    match protocol {
        Protocol::Chat => match event {
            ModelEvent::TextDelta(delta) => vec![(
                "message",
                chat_chunk(id, model, json!({"content":delta}), Value::Null),
            )],
            ModelEvent::ToolCallStart {
                index,
                call_id,
                name,
            } => vec![(
                "message",
                chat_chunk(
                    id,
                    model,
                    json!({"tool_calls":[{"index":index,"id":call_id,"type":"function","function":{"name":name,"arguments":""}}]}),
                    Value::Null,
                ),
            )],
            ModelEvent::ToolCallArgumentsDelta { index, delta } => vec![(
                "message",
                chat_chunk(
                    id,
                    model,
                    json!({"tool_calls":[{"index":index,"function":{"arguments":delta}}]}),
                    Value::Null,
                ),
            )],
            ModelEvent::Done(reason) => {
                let finish = match reason {
                    FinishReason::Stop => "stop",
                    FinishReason::Length => "length",
                    FinishReason::ToolUse => "tool_calls",
                };
                let mut events = vec![("message", chat_chunk(id, model, json!({}), json!(finish)))];
                if output.usage.is_some() {
                    events.push((
                        "message",
                        json!({"id":id,"object":"chat.completion.chunk","created":0,"model":model,
                            "choices":[],"usage":output.usage_json(protocol)}),
                    ));
                }
                events
            }
            _ => Vec::new(),
        },
        Protocol::Responses => match event {
            ModelEvent::TextStart if !*text_started => {
                *text_started = true;
                vec![
                    (
                        "response.output_item.added",
                        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":format!("msg_{id}"),"status":"in_progress","role":"assistant","content":[]}}),
                    ),
                    (
                        "response.content_part.added",
                        json!({"type":"response.content_part.added","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
                    ),
                ]
            }
            ModelEvent::TextDelta(delta) => vec![(
                "response.output_text.delta",
                json!({"type":"response.output_text.delta","delta":delta,"output_index":0,"content_index":0}),
            )],
            ModelEvent::TextEnd if *text_started => vec![
                (
                    "response.output_text.done",
                    json!({"type":"response.output_text.done","text":output.text,"output_index":0,"content_index":0}),
                ),
                (
                    "response.content_part.done",
                    json!({"type":"response.content_part.done","output_index":0,"content_index":0,"part":{"type":"output_text","text":output.text,"annotations":[]}}),
                ),
                (
                    "response.output_item.done",
                    json!({"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":format!("msg_{id}"),"status":"completed","role":"assistant","content":[{"type":"output_text","text":output.text,"annotations":[]}]}}),
                ),
            ],
            ModelEvent::ToolCallStart {
                index,
                call_id,
                name,
            } => vec![(
                "response.output_item.added",
                json!({"type":"response.output_item.added","output_index":index + usize::from(*text_started),"item":{"type":"function_call","id":format!("fc_{call_id}"),"call_id":call_id,"name":name,"arguments":"","status":"in_progress"}}),
            )],
            ModelEvent::ToolCallArgumentsDelta { index, delta } => vec![(
                "response.function_call_arguments.delta",
                json!({"type":"response.function_call_arguments.delta","output_index":index + usize::from(*text_started),"delta":delta}),
            )],
            ModelEvent::ToolCallEnd { index } => {
                let tool = output.tools.get(index);
                let arguments = tool.map(|tool| tool.arguments.as_str()).unwrap_or_default();
                let call_id = tool.map(|tool| tool.id.as_str()).unwrap_or_default();
                vec![
                    (
                        "response.function_call_arguments.done",
                        json!({"type":"response.function_call_arguments.done","output_index":index + usize::from(*text_started),"arguments":arguments}),
                    ),
                    (
                        "response.output_item.done",
                        json!({"type":"response.output_item.done","output_index":index + usize::from(*text_started),"item":{"type":"function_call","id":format!("fc_{call_id}"),"call_id":call_id,"name":tool.map(|tool| tool.name.as_str()).unwrap_or("tool"),"arguments":arguments,"status":"completed"}}),
                    ),
                ]
            }
            ModelEvent::Done(_) => vec![(
                "response.completed",
                json!({"type":"response.completed","response":output.json(protocol,id,model)}),
            )],
            _ => Vec::new(),
        },
        Protocol::Messages => match event {
            ModelEvent::TextStart => {
                if *text_started {
                    Vec::new()
                } else {
                    *text_started = true;
                    vec![(
                        "content_block_start",
                        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                    )]
                }
            }
            ModelEvent::TextDelta(delta) => {
                let mut events = Vec::new();
                if !*text_started {
                    *text_started = true;
                    events.push(("content_block_start",json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})));
                }
                events.push(("content_block_delta",json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":delta}})));
                events
            }
            ModelEvent::TextEnd => {
                if *text_started {
                    vec![(
                        "content_block_stop",
                        json!({"type":"content_block_stop","index":0}),
                    )]
                } else {
                    Vec::new()
                }
            }
            ModelEvent::ToolCallStart {
                index,
                call_id,
                name,
            } => vec![(
                "content_block_start",
                json!({"type":"content_block_start","index":index + usize::from(*text_started),"content_block":{"type":"tool_use","id":call_id,"name":name,"input":{}}}),
            )],
            ModelEvent::ToolCallArgumentsDelta { index, delta } => vec![(
                "content_block_delta",
                json!({"type":"content_block_delta","index":index + usize::from(*text_started),"delta":{"type":"input_json_delta","partial_json":delta}}),
            )],
            ModelEvent::ToolCallEnd { index } => vec![(
                "content_block_stop",
                json!({"type":"content_block_stop","index":index + usize::from(*text_started)}),
            )],
            ModelEvent::Done(reason) => {
                let stop = match reason {
                    FinishReason::Stop => "end_turn",
                    FinishReason::Length => "max_tokens",
                    FinishReason::ToolUse => "tool_use",
                };
                vec![
                    (
                        "message_delta",
                        json!({"type":"message_delta","delta":{"stop_reason":stop,"stop_sequence":null},"usage":output.usage_json(protocol)}),
                    ),
                    ("message_stop", json!({"type":"message_stop"})),
                ]
            }
            _ => Vec::new(),
        },
    }
}
