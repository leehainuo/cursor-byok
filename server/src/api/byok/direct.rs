//! Forwards matching HTTP protocols without projecting provider messages.
use std::time::Duration;

use async_stream::try_stream;
use axum::{
    body::Body,
    http::{header, HeaderMap, StatusCode},
    response::Response,
};
use bytes::Bytes;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::Value;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    model::{ModelConfig, NewLlmCall, Usage},
    network::NetworkClients,
    provider::{custom_headers, CallRecorder, FinishReason},
    store::Store,
    Error, Result,
};

use super::protocol::Protocol;

#[derive(Clone)]
pub struct NativeForwarder {
    store: Store,
    clients: NetworkClients,
    request_timeout: Duration,
    stream_idle_timeout: Duration,
}

impl NativeForwarder {
    pub fn new(
        store: Store,
        clients: NetworkClients,
        request_timeout: Duration,
        stream_idle_timeout: Duration,
    ) -> Self {
        Self {
            store,
            clients,
            request_timeout,
            stream_idle_timeout,
        }
    }

    pub(super) async fn forward(
        &self,
        protocol: Protocol,
        incoming_headers: &HeaderMap,
        mut body: Value,
        model: &ModelConfig,
    ) -> Result<Response> {
        let call_id = format!("external-api:{}", uuid::Uuid::new_v4());
        let request_url = model.request_url()?;
        let new_call = NewLlmCall {
            call_id: call_id.clone(),
            run_id: call_id.clone(),
            conversation_id: call_id.clone(),
            provider_call_index: 0,
            model_hash: model.model_hash.clone(),
            provider_type: model.provider_type(),
            provider_url: request_url.clone(),
            request_type: model.provider_type(),
            request_url: request_url.clone(),
            model_id: model.model_id.clone(),
            display_name: model.display_name.clone(),
            reasoning_effort: model.reasoning_effort.clone(),
            fast: false,
            message_count: body
                .get("messages")
                .or_else(|| body.get("input"))
                .and_then(Value::as_array)
                .map_or(1, Vec::len),
            tool_count: body
                .get("tools")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
            detailed: false,
        };
        body["model"] = Value::String(model.model_id.clone());
        let mut upstream_headers = HeaderMap::new();
        for (name, value) in incoming_headers {
            if !matches!(
                name.as_str(),
                "authorization"
                    | "x-api-key"
                    | "host"
                    | "content-length"
                    | "content-type"
                    | "connection"
                    | "transfer-encoding"
                    | "accept-encoding"
                    | "cookie"
                    | "proxy-authorization"
                    | "proxy-authenticate"
                    | "te"
                    | "trailer"
                    | "upgrade"
            ) {
                upstream_headers.append(name, value.clone());
            }
        }
        if model.custom_headers_enabled {
            upstream_headers.extend(custom_headers(&model.custom_headers, &call_id)?);
        }
        match protocol {
            Protocol::Messages => {
                upstream_headers.insert(
                    "x-api-key",
                    model.api_key.parse().map_err(|error| {
                        Error::Config(format!("invalid API key header: {error}"))
                    })?,
                );
                if !upstream_headers.contains_key("anthropic-version") {
                    upstream_headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
                }
            }
            Protocol::Chat | Protocol::Responses => {
                upstream_headers.insert(
                    header::AUTHORIZATION,
                    format!("Bearer {}", model.api_key)
                        .parse()
                        .map_err(|error| {
                            Error::Config(format!("invalid API key header: {error}"))
                        })?,
                );
            }
        }
        let logged_headers = upstream_headers
            .iter()
            .filter(|(name, _)| !crate::model::is_sensitive_header(name.as_str()))
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_owned(), Value::String(value.to_owned())))
            })
            .collect::<serde_json::Map<_, _>>();
        let client = self.clients.provider_client(self.request_timeout).await?;
        let recorder = CallRecorder::start(self.store.clone(), new_call).await?;
        recorder
            .request(Value::Object(logged_headers), &body)
            .await?;
        let response = match client
            .post(&request_url)
            .headers(upstream_headers)
            .json(&body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let error = Error::Http(error);
                recorder.failed(&error).await?;
                return Err(error);
            }
        };
        let status = response.status();
        recorder.response_headers(status.as_u16()).await?;
        let headers = response.headers().clone();
        let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
        if stream && status.is_success() {
            Ok(self.stream_response(protocol, response, recorder, status, &headers))
        } else {
            let bytes = match response.bytes().await {
                Ok(bytes) => bytes,
                Err(error) => {
                    let error = Error::Http(error);
                    recorder.failed(&error).await?;
                    return Err(error);
                }
            };
            recorder.response_chunk(&bytes).await?;
            if status.is_success() {
                if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                    let summary = NativeSummary::from_value(protocol, &value);
                    if let Some(usage) = summary.usage {
                        recorder.usage(usage).await?;
                    }
                    if let Some(message) = summary.failure {
                        recorder.failed(&Error::Provider(message)).await?;
                    } else {
                        recorder
                            .completed(summary.finish.unwrap_or(FinishReason::Stop))
                            .await?;
                    }
                } else {
                    recorder.completed(FinishReason::Stop).await?;
                }
            } else {
                recorder
                    .failed(&Error::Provider(format!("upstream returned {status}")))
                    .await?;
            }
            Ok(build_response(status, &headers, Body::from(bytes)))
        }
    }

    fn stream_response(
        &self,
        protocol: Protocol,
        response: reqwest::Response,
        recorder: CallRecorder,
        status: StatusCode,
        headers: &HeaderMap,
    ) -> Response {
        let idle_timeout = self.stream_idle_timeout;
        let output = try_stream! {
            let _cancel = recorder.cancel_on_drop();
            let (sender, receiver) = tokio::sync::mpsc::channel::<Bytes>(8);
            let observer = tokio::spawn(observe_events(protocol, receiver));
            let mut upstream = response.bytes_stream();
            loop {
                let next = tokio::time::timeout(idle_timeout, upstream.next()).await;
                let chunk = match next {
                    Ok(Some(Ok(chunk))) => chunk,
                    Ok(None) => break,
                    Ok(Some(Err(error))) => {
                        let error = Error::Http(error);
                        recorder.failed(&error).await?;
                        Err::<Bytes, Error>(error)?
                    }
                    Err(_) => {
                        let error = Error::Provider("native upstream stream idle timeout".into());
                        recorder.failed(&error).await?;
                        Err::<Bytes, Error>(error)?
                    }
                };
                recorder.response_chunk(&chunk).await?;
                let _ = sender.send(chunk.clone()).await;
                yield chunk;
            }
            drop(sender);
            if let Ok(summary) = observer.await {
                if let Some(usage) = summary.usage { recorder.usage(usage).await?; }
                if let Some(message) = summary.failure {
                    recorder.failed(&Error::Provider(message)).await?;
                } else {
                    recorder.completed(summary.finish.unwrap_or(FinishReason::Stop)).await?;
                }
            } else {
                recorder.completed(FinishReason::Stop).await?;
            }
        };
        let output = output.map(|result: Result<Bytes>| result);
        build_response(status, headers, Body::from_stream(output))
    }
}

fn build_response(status: StatusCode, headers: &HeaderMap, body: Body) -> Response {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    for (name, value) in headers {
        if !matches!(
            name.as_str(),
            "content-length" | "content-encoding" | "transfer-encoding" | "connection"
        ) {
            response.headers_mut().append(name, value.clone());
        }
    }
    response
}

#[derive(Default)]
struct NativeSummary {
    usage: Option<Usage>,
    finish: Option<FinishReason>,
    failure: Option<String>,
}

impl NativeSummary {
    fn from_value(protocol: Protocol, value: &Value) -> Self {
        let mut summary = Self::default();
        summary.observe(protocol, value);
        summary
    }

    fn observe(&mut self, protocol: Protocol, value: &Value) {
        if matches!(
            value.get("type").and_then(Value::as_str),
            Some("error" | "response.failed")
        ) || value.get("error").is_some_and(|error| !error.is_null())
        {
            self.failure = Some(
                value
                    .pointer("/error/message")
                    .or_else(|| value.pointer("/response/error/message"))
                    .or_else(|| value.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("upstream error event")
                    .to_owned(),
            );
        }
        let usage = match protocol {
            Protocol::Chat => value.get("usage"),
            Protocol::Responses => value
                .pointer("/response/usage")
                .or_else(|| value.get("usage")),
            Protocol::Messages => value
                .pointer("/message/usage")
                .or_else(|| value.get("usage")),
        }
        .filter(|usage| !usage.is_null())
        .map(|usage| crate::provider::native_usage(protocol.provider_type(), usage));
        if let Some(usage) = usage {
            let current = self.usage.get_or_insert_default();
            for (target, incoming) in [
                (&mut current.input_tokens, usage.input_tokens),
                (
                    &mut current.context_input_tokens,
                    usage.context_input_tokens,
                ),
                (&mut current.output_tokens, usage.output_tokens),
                (&mut current.total_tokens, usage.total_tokens),
                (&mut current.cache_read_tokens, usage.cache_read_tokens),
                (&mut current.cache_write_tokens, usage.cache_write_tokens),
                (&mut current.reasoning_tokens, usage.reasoning_tokens),
            ] {
                if incoming.is_some() {
                    *target = incoming;
                }
            }
        }
        let reason = match protocol {
            Protocol::Chat => value
                .pointer("/choices/0/finish_reason")
                .and_then(Value::as_str),
            Protocol::Responses => value
                .pointer("/response/incomplete_details/reason")
                .and_then(Value::as_str),
            Protocol::Messages => value
                .pointer("/delta/stop_reason")
                .or_else(|| value.get("stop_reason"))
                .and_then(Value::as_str),
        };
        self.finish = match reason {
            Some("tool_calls" | "tool_use") => Some(FinishReason::ToolUse),
            Some("length" | "max_tokens" | "max_output_tokens") => Some(FinishReason::Length),
            Some(_) => Some(FinishReason::Stop),
            None => self.finish,
        };
        if matches!(protocol, Protocol::Responses)
            && (value.pointer("/item/type").and_then(Value::as_str) == Some("function_call")
                || value
                    .pointer("/response/output")
                    .and_then(Value::as_array)
                    .is_some_and(|items| {
                        items.iter().any(|item| {
                            item.get("type").and_then(Value::as_str) == Some("function_call")
                        })
                    }))
        {
            self.finish = Some(FinishReason::ToolUse);
        }
    }
}

async fn observe_events(
    protocol: Protocol,
    receiver: tokio::sync::mpsc::Receiver<Bytes>,
) -> NativeSummary {
    let source = ReceiverStream::new(receiver)
        .map(Ok::<_, Error>)
        .eventsource();
    futures_util::pin_mut!(source);
    let mut summary = NativeSummary::default();
    while let Some(event) = source.next().await {
        let Ok(event) = event else {
            break;
        };
        if let Ok(value) = serde_json::from_str::<Value>(&event.data) {
            summary.observe(protocol, &value);
        }
    }
    summary
}
