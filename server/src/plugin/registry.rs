//! Orchestrates plugin capabilities: resources, model catalogs, and invocation.
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use async_stream::try_stream;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use super::{
    catalog::{PluginCatalog, PluginEntry},
    data::PluginDataStore,
    descriptor::{
        parse_model_id, PluginDescriptor, PluginModelDescriptor, PluginProviderDescriptor,
        PluginResourceDescriptor, PluginResourceView, ProviderDefinition, ResourceActionResponse,
        ResourceActionResult, ResourceDefinition, ResourcePresentation, FORM_ADD_METHOD,
        FORM_FIELD_URL, LIVE_CATALOG, OAUTH2_ADD_METHOD, OAUTH2_AUTHORIZATION_CODE_ADD_METHOD,
        SNAPSHOT_CATALOG,
    },
    oauth_callback::{self, CallbackHandle, CallbackOutcome, CallbackRequest},
    origin,
    runtime::PluginRuntime,
    state::{now_ms, PluginStateStore, ResourceDraft, ResourcePatch, ResourceRecord, StoredModel},
    wire,
    worker::{PluginWorker, WorkerStreamItem},
};
use crate::{
    model::ModelInvocation,
    provider::ProviderStream,
    provider::{CallRecorder, ModelEvent},
    store::Store,
    Error, Result,
};

const OAUTH_SLOW_DOWN_STEP_MS: i64 = 5_000;
const MAX_IMPORT_DRAFTS: usize = 256;
const LIVE_CATALOG_TTL: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct PluginRegistry {
    inner: Arc<RegistryInner>,
}

struct RegistryInner {
    store: Store,
    runtime: PluginRuntime,
    catalog: PluginCatalog,
    state: PluginStateStore,
    entries: RwLock<Option<Vec<PluginEntry>>>,
    workers: Mutex<HashMap<String, Arc<PluginWorker>>>,
    oauth_sessions: Mutex<HashMap<String, OAuthSession>>,
    live_catalogs: Mutex<HashMap<LiveCatalogKey, LiveCatalogEntry>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LiveCatalogKey {
    plugin_id: String,
    provider_id: String,
    resource_id: String,
}

#[derive(Clone, Debug)]
struct LiveCatalogEntry {
    models: Vec<StoredModel>,
    fetched_at: Instant,
    error: Option<String>,
}

struct OAuthSession {
    plugin_id: String,
    resource_type: String,
    method_id: String,
    session: serde_json::Value,
    expires_at_ms: i64,
    poll_interval_ms: i64,
    next_poll_at_ms: i64,
    flow: OAuthFlow,
}

enum OAuthFlow {
    DeviceCode,
    AuthorizationCode {
        redirect_uri: String,
        code_verifier: String,
        callback: CallbackHandle,
    },
}

enum OAuthPollWork {
    DeviceCode {
        plugin_id: String,
        resource_type: String,
        method_id: String,
        session: serde_json::Value,
        poll_interval_ms: i64,
    },
    AuthorizationCode {
        plugin_id: String,
        resource_type: String,
        method_id: String,
        session: serde_json::Value,
        redirect_uri: String,
        code_verifier: String,
        callback_request: CallbackRequest,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthBeginResponse {
    pub session_id: String,
    pub user_code: Option<String>,
    pub verification_url: String,
    pub verification_url_complete: Option<String>,
    pub expires_at_ms: i64,
    pub poll_interval_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum OAuthPollResponse {
    #[serde(rename_all = "camelCase")]
    Pending { poll_interval_ms: i64 },
    #[serde(rename_all = "camelCase")]
    Completed {
        added: usize,
        updated: usize,
        model_sync_error: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Denied { message: Option<String> },
    #[serde(rename_all = "camelCase")]
    Failed { message: String },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResponse {
    pub added: usize,
    pub updated: usize,
    pub warnings: Vec<String>,
    pub model_sync_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormSubmitResponse {
    pub added: usize,
    pub updated: usize,
    pub model_sync_error: Option<String>,
}

/// 路由分支在建立 Recorder 时需要的插件模型元数据。
#[derive(Clone, Debug)]
pub struct PluginInvocationPlan {
    pub model: PluginModelDescriptor,
    pub request_url: String,
}

impl PluginRegistry {
    pub fn managed(store: Store, runtime: PluginRuntime, app_version: String) -> Result<Self> {
        let data = PluginDataStore::managed()?;
        Ok(Self {
            inner: Arc::new(RegistryInner {
                store,
                runtime,
                catalog: PluginCatalog::managed(app_version)?,
                state: PluginStateStore::new(data),
                entries: RwLock::new(None),
                workers: Mutex::new(HashMap::new()),
                oauth_sessions: Mutex::new(HashMap::new()),
                live_catalogs: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub async fn plugins(&self) -> Vec<PluginDescriptor> {
        let Some(executable) = self.inner.runtime.executable() else {
            return self
                .inner
                .catalog
                .manifests()
                .into_iter()
                .map(|(manifest, icon)| PluginDescriptor {
                    id: manifest.id,
                    name: manifest.name,
                    version: manifest.version,
                    author: manifest.author,
                    icon,
                    providers: Vec::new(),
                    resources: Vec::new(),
                })
                .collect();
        };
        let mut plugins = Vec::new();
        for entry in self.entries(&executable).await {
            plugins.push(self.descriptor(&entry, &executable).await);
        }
        plugins
    }

    /// 已满足调用条件的全部插件模型;每个模型独立进入 Cursor 目录。
    pub async fn configured_models(&self) -> Vec<PluginModelDescriptor> {
        let Some(executable) = self.inner.runtime.executable() else {
            return Vec::new();
        };
        let mut models = Vec::new();
        for entry in self.entries(&executable).await {
            for provider in &entry.definition.providers {
                if !self.provider_configured(&entry, provider).await {
                    continue;
                }
                models.extend(
                    self.provider_models(&entry, &executable, provider, false)
                        .await
                        .into_iter()
                        .filter(|model| model.enabled)
                        .map(|model| {
                            PluginModelDescriptor::new(
                                &entry.manifest.id,
                                &entry.manifest.name,
                                &entry.icon,
                                provider,
                                &model,
                            )
                        }),
                );
            }
        }
        models
    }

    pub async fn model_descriptor(&self, model_id: &str) -> Result<PluginModelDescriptor> {
        let (plugin_id, provider_id, stored_id) = parse_model_id(model_id)
            .ok_or_else(|| Error::Provider(format!("invalid plugin model ID: {model_id}")))?;
        let executable = self.executable()?;
        let entry = self.find_entry(&executable, plugin_id).await?;
        let provider = find_provider(&entry, provider_id)?;
        let stored = self
            .provider_models(&entry, &executable, provider, false)
            .await
            .into_iter()
            .find(|model| model.id == stored_id)
            .ok_or_else(|| Error::RunNotFound(format!("plugin model {model_id}")))?;
        Ok(PluginModelDescriptor::new(
            plugin_id,
            &entry.manifest.name,
            &entry.icon,
            provider,
            &stored,
        ))
    }

    pub async fn plan_model(&self, model_id: &str) -> Result<PluginInvocationPlan> {
        let model = self.model_descriptor(model_id).await?;
        let request_url = format!("plugin://{}/{}", model.plugin_id, model.provider_id);
        Ok(PluginInvocationPlan { model, request_url })
    }

    /// 插件模型的统一 Provider 流:选首个可用资源,经 Worker 执行,
    /// 事件与内置 Provider 走同一管道。未来的负载均衡在这里换资源重试。
    pub fn stream_model(
        &self,
        invocation: ModelInvocation,
        cancellation: CancellationToken,
        recorder: CallRecorder,
    ) -> ProviderStream {
        let registry = self.clone();
        Box::pin(try_stream! {
            let model_id = invocation.request.model.model_id.clone();
            let (plugin_id, provider_id, upstream_id) = parse_model_id(&model_id)
                .map(|(plugin, provider, model)| (plugin.to_owned(), provider.to_owned(), model.to_owned()))
                .ok_or_else(|| Error::Provider(format!("invalid plugin model ID: {model_id}")))?;
            let executable = registry.executable()?;
            let entry = registry.find_entry(&executable, &plugin_id).await?;
            let provider = find_provider(&entry, &provider_id)?.clone();
            let stored = registry
                .provider_models(&entry, &executable, &provider, false)
                .await
                .into_iter()
                .find(|model| model.id == upstream_id)
                .ok_or_else(|| Error::RunNotFound(format!("plugin model {model_id}")))?;
            let resource = match &provider.resource_type {
                Some(resource_type) if provider.catalog == LIVE_CATALOG => {
                    let resource_id = live_resource_id(&upstream_id).ok_or_else(|| {
                        Error::Provider(format!("invalid live plugin model ID: {model_id}"))
                    })?;
                    Some((
                        resource_type.clone(),
                        registry
                            .find_record(&plugin_id, resource_type, resource_id)
                            .await?,
                    ))
                }
                Some(resource_type) => Some((
                    resource_type.clone(),
                    registry.select_resource(&plugin_id, resource_type).await?,
                )),
                None => None,
            };
            let mut model_snapshot = stored.snapshot();
            if provider.catalog == LIVE_CATALOG {
                if let Some(upstream) = live_upstream_id(&upstream_id) {
                    if let Some(object) = model_snapshot.as_object_mut() {
                        object.insert("id".into(), serde_json::Value::String(upstream.to_owned()));
                    }
                }
            }
            let request = wire::llm_request(&invocation)?;
            let params = serde_json::json!({
                "providerId": provider_id,
                "model": model_snapshot,
                "resource": resource.as_ref().map(|(resource_type, record)| record.snapshot(resource_type)),
                "request": request,
            });
            let worker = registry.worker(&entry, &executable).await;
            let origins = registry.resource_origins(&entry, resource.as_ref().map(|(_, record)| record));
            let mut items = worker.invoke_streaming_with_origins("provider.invoke", params, cancellation.clone(), Some(recorder), origins).await?;
            yield ModelEvent::Start { model_call_id: invocation.call_id.clone() };
            while let Some(item) = items.recv().await {
                match item {
                    WorkerStreamItem::Event(event) => {
                        yield wire::model_event(&event)?;
                    }
                    WorkerStreamItem::Result(result) => {
                        let value = result?;
                        let status = value.get("status").and_then(serde_json::Value::as_str).unwrap_or_default();
                        let patch = value.get("patch")
                            .filter(|patch| !patch.is_null())
                            .map(|patch| serde_json::from_value::<ResourcePatch>(patch.clone()))
                            .transpose()?;
                        if let (Some(patch), Some((resource_type, record))) = (patch, resource.as_ref()) {
                            if let Err(error) = registry.inner.state
                                .apply_patch(&plugin_id, resource_type, &record.id, patch).await
                            {
                                tracing::warn!(plugin = %plugin_id, %error, "failed to apply plugin resource patch");
                            } else {
                                registry.invalidate_live_catalogs(&plugin_id, Some(&record.id)).await;
                            }
                        }
                        match status {
                            "completed" => return,
                            "resource-error" | "request-error" => {
                                let message = value.get("message")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("plugin provider call failed");
                                Err(Error::Provider(message.to_owned()))?;
                            }
                            status => {
                                Err(Error::Protocol(format!("unknown plugin provider result: {status}")))?;
                            }
                        }
                    }
                }
            }
            Err(Error::Provider(format!("plugin '{plugin_id}' worker stopped mid-stream")))?;
        })
    }

    pub async fn oauth_begin(
        &self,
        plugin_id: &str,
        resource_type: &str,
        method_id: &str,
    ) -> Result<OAuthBeginResponse> {
        let executable = self.executable()?;
        let entry = self.find_entry(&executable, plugin_id).await?;
        let resource = find_resource(&entry, resource_type)?;
        let method = resource
            .add
            .iter()
            .find(|method| method.id == method_id)
            .ok_or_else(|| {
                Error::Config(format!(
                    "plugin '{plugin_id}' does not define OAuth method '{method_id}'"
                ))
            })?;
        // 同一添加入口只有一个活跃生命周期。重新开始时先丢弃旧会话，
        // Drop 授权码会话中的 CallbackHandle 会立即释放 loopback listener。
        self.inner.oauth_sessions.lock().await.retain(|_, session| {
            session.plugin_id != plugin_id
                || session.resource_type != resource_type
                || session.method_id != method_id
        });
        let worker = self.worker(&entry, &executable).await;
        let session_id = uuid::Uuid::new_v4().to_string();

        let (
            session,
            verification_url,
            verification_url_complete,
            expires_at_ms,
            poll_interval_ms,
            flow,
            user_code,
        ) = match method.method_type.as_str() {
            OAUTH2_ADD_METHOD => {
                let value = worker
                    .invoke(
                        "oauth.begin",
                        serde_json::json!({
                            "resourceType": resource_type,
                            "methodId": method.id,
                        }),
                        CancellationToken::new(),
                    )
                    .await?;
                let begin: OAuth2Begin = serde_json::from_value(value)?;
                (
                    begin.session,
                    begin.verification_url,
                    begin.verification_url_complete,
                    begin.expires_at_ms,
                    begin.poll_interval_ms.max(1_000),
                    OAuthFlow::DeviceCode,
                    Some(begin.user_code),
                )
            }
            OAUTH2_AUTHORIZATION_CODE_ADD_METHOD => {
                let state = oauth_random_secret();
                let code_verifier = oauth_random_secret();
                let code_challenge =
                    URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
                let callback = method.callback.as_ref();
                let callback = oauth_callback::bind(
                    callback.and_then(|value| value.port),
                    callback
                        .and_then(|value| value.path.as_deref())
                        .unwrap_or("/oauth-callback"),
                    state.clone(),
                    entry.manifest.name.clone(),
                    entry.icon.clone(),
                    serde_json::to_value(&resource.display_name)?,
                )
                .await?;
                let redirect_uri = callback.redirect_uri.clone();
                let value = worker
                    .invoke(
                        "oauth.begin",
                        serde_json::json!({
                            "resourceType": resource_type,
                            "methodId": method.id,
                            "authorization": {
                                "redirectUri": redirect_uri,
                                "state": state,
                                "codeChallenge": code_challenge,
                            },
                        }),
                        CancellationToken::new(),
                    )
                    .await?;
                let begin: OAuth2AuthorizationCodeBegin = serde_json::from_value(value)?;
                let poll_interval_ms = begin.poll_interval_ms.unwrap_or(1_000).max(1_000);
                (
                    begin.session,
                    begin.authorization_url,
                    None,
                    begin.expires_at_ms,
                    poll_interval_ms,
                    OAuthFlow::AuthorizationCode {
                        redirect_uri,
                        code_verifier,
                        callback,
                    },
                    None,
                )
            }
            method_type => {
                return Err(Error::Config(format!(
                        "plugin '{plugin_id}' OAuth method '{method_id}' uses unsupported type '{method_type}'"
                    )));
            }
        };

        if expires_at_ms <= now_ms() {
            return Err(Error::Protocol(format!(
                "plugin '{plugin_id}' OAuth method '{method_id}' returned an expired session"
            )));
        }
        self.inner.oauth_sessions.lock().await.insert(
            session_id.clone(),
            OAuthSession {
                plugin_id: plugin_id.to_owned(),
                resource_type: resource_type.to_owned(),
                method_id: method_id.to_owned(),
                session,
                expires_at_ms,
                poll_interval_ms,
                next_poll_at_ms: now_ms() + poll_interval_ms,
                flow,
            },
        );
        let cleanup = self.clone();
        let cleanup_session_id = session_id.clone();
        let cleanup_delay_ms = expires_at_ms.saturating_sub(now_ms()) as u64;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(cleanup_delay_ms)).await;
            cleanup
                .inner
                .oauth_sessions
                .lock()
                .await
                .remove(&cleanup_session_id);
        });
        Ok(OAuthBeginResponse {
            session_id,
            user_code,
            verification_url,
            verification_url_complete,
            expires_at_ms,
            poll_interval_ms,
        })
    }

    pub async fn form_submit(
        &self,
        plugin_id: &str,
        resource_type: &str,
        method_id: &str,
        values: serde_json::Value,
    ) -> Result<FormSubmitResponse> {
        let executable = self.executable()?;
        let entry = self.find_entry(&executable, plugin_id).await?;
        let resource = find_resource(&entry, resource_type)?;
        let method = resource
            .add
            .iter()
            .find(|method| method.id == method_id)
            .ok_or_else(|| {
                Error::Config(format!(
                    "plugin '{plugin_id}' does not define form method '{method_id}'"
                ))
            })?;
        if method.method_type != FORM_ADD_METHOD {
            return Err(Error::Config(format!(
                "plugin '{plugin_id}' add method '{method_id}' is not a form method"
            )));
        }
        let values = values
            .as_object()
            .cloned()
            .ok_or_else(|| Error::Config("form values must be an object".into()))?;
        let mut submitted = serde_json::Map::new();
        let mut submit_origins = HashSet::new();
        for field in &method.fields {
            let raw = values
                .get(&field.id)
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim();
            if field.required && raw.is_empty() {
                return Err(Error::Config(format!(
                    "form field '{}' is required",
                    field.id
                )));
            }
            if field.field_type == FORM_FIELD_URL && !raw.is_empty() {
                submit_origins.insert(origin::origin_from_url(raw)?);
            }
            submitted.insert(field.id.clone(), serde_json::Value::String(raw.to_owned()));
        }
        let value = self
            .worker(&entry, &executable)
            .await
            .invoke_with_origins(
                "form.submit",
                serde_json::json!({
                    "resourceType": resource_type,
                    "methodId": method.id,
                    "values": submitted,
                }),
                CancellationToken::new(),
                submit_origins,
            )
            .await?;
        let mut resources: Vec<ResourceDraft> = serde_json::from_value(value)?;
        if resources.is_empty() {
            return Err(Error::Protocol("form submit produced no resources".into()));
        }
        for draft in &mut resources {
            let origin = origin::origin_from_private_data(&draft.private_data)?;
            origin::rewrite_private_origin(&mut draft.private_data, origin);
        }
        let outcome = self
            .persist_resources(&entry, plugin_id, resource_type, resources)
            .await?;
        let model_sync_error = self
            .sync_provider_models_for_resource(&entry, &executable, resource_type)
            .await;
        Ok(FormSubmitResponse {
            added: outcome.added,
            updated: outcome.updated,
            model_sync_error,
        })
    }

    pub async fn oauth_poll(&self, session_id: &str) -> Result<OAuthPollResponse> {
        let work = {
            let now = now_ms();
            let mut sessions = self.inner.oauth_sessions.lock().await;
            let Some(state) = sessions.get_mut(session_id) else {
                return Ok(OAuthPollResponse::Failed {
                    message: "authorization session no longer exists".into(),
                });
            };
            if now >= state.expires_at_ms {
                sessions.remove(session_id);
                return Ok(OAuthPollResponse::Failed {
                    message: "authorization expired".into(),
                });
            }
            if now < state.next_poll_at_ms {
                return Ok(OAuthPollResponse::Pending {
                    poll_interval_ms: state.poll_interval_ms,
                });
            }
            state.next_poll_at_ms = now + state.poll_interval_ms;

            let common = (
                state.plugin_id.clone(),
                state.resource_type.clone(),
                state.method_id.clone(),
                state.session.clone(),
            );
            match &mut state.flow {
                OAuthFlow::DeviceCode => OAuthPollWork::DeviceCode {
                    plugin_id: common.0,
                    resource_type: common.1,
                    method_id: common.2,
                    session: common.3,
                    poll_interval_ms: state.poll_interval_ms,
                },
                OAuthFlow::AuthorizationCode {
                    redirect_uri,
                    code_verifier,
                    callback,
                } => match callback.receiver.try_recv() {
                    Ok(callback_request) => OAuthPollWork::AuthorizationCode {
                        plugin_id: common.0,
                        resource_type: common.1,
                        method_id: common.2,
                        session: common.3,
                        redirect_uri: redirect_uri.clone(),
                        code_verifier: code_verifier.clone(),
                        callback_request,
                    },
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                        return Ok(OAuthPollResponse::Pending {
                            poll_interval_ms: state.poll_interval_ms,
                        });
                    }
                    Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                        sessions.remove(session_id);
                        return Ok(OAuthPollResponse::Failed {
                            message: "authorization callback stopped before completion".into(),
                        });
                    }
                },
            }
        };

        match work {
            OAuthPollWork::DeviceCode {
                plugin_id,
                resource_type,
                method_id,
                session,
                poll_interval_ms,
            } => {
                self.poll_device_code(
                    session_id,
                    plugin_id,
                    resource_type,
                    method_id,
                    session,
                    poll_interval_ms,
                )
                .await
            }
            OAuthPollWork::AuthorizationCode {
                plugin_id,
                resource_type,
                method_id,
                session,
                redirect_uri,
                code_verifier,
                callback_request,
            } => {
                self.complete_authorization_code(
                    session_id,
                    plugin_id,
                    resource_type,
                    method_id,
                    session,
                    redirect_uri,
                    code_verifier,
                    callback_request,
                )
                .await
            }
        }
    }

    async fn poll_device_code(
        &self,
        session_id: &str,
        plugin_id: String,
        resource_type: String,
        method_id: String,
        session: serde_json::Value,
        poll_interval_ms: i64,
    ) -> Result<OAuthPollResponse> {
        let executable = self.executable()?;
        let entry = self.find_entry(&executable, &plugin_id).await?;
        let value = self
            .worker(&entry, &executable)
            .await
            .invoke(
                "oauth.poll",
                serde_json::json!({
                    "resourceType": resource_type,
                    "methodId": method_id,
                    "session": session,
                }),
                CancellationToken::new(),
            )
            .await?;
        let poll: OAuth2Poll = serde_json::from_value(value)?;
        match poll {
            OAuth2Poll::Pending { session } => {
                self.update_session(session_id, session, None).await;
                Ok(OAuthPollResponse::Pending { poll_interval_ms })
            }
            OAuth2Poll::SlowDown { session } => {
                let interval = poll_interval_ms + OAUTH_SLOW_DOWN_STEP_MS;
                self.update_session(session_id, session, Some(interval))
                    .await;
                Ok(OAuthPollResponse::Pending {
                    poll_interval_ms: interval,
                })
            }
            OAuth2Poll::Completed { resources } => {
                // 持久化成功后才销毁设备码会话:写盘瞬时失败时下次轮询还能重试。
                let response = self
                    .persist_oauth_resources(
                        &entry,
                        &executable,
                        &plugin_id,
                        &resource_type,
                        resources,
                    )
                    .await?;
                self.inner.oauth_sessions.lock().await.remove(session_id);
                Ok(response)
            }
            OAuth2Poll::Denied { message } => {
                self.inner.oauth_sessions.lock().await.remove(session_id);
                Ok(OAuthPollResponse::Denied { message })
            }
            OAuth2Poll::Failed { message } => {
                self.inner.oauth_sessions.lock().await.remove(session_id);
                Ok(OAuthPollResponse::Failed { message })
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn complete_authorization_code(
        &self,
        session_id: &str,
        plugin_id: String,
        resource_type: String,
        method_id: String,
        session: serde_json::Value,
        redirect_uri: String,
        code_verifier: String,
        callback_request: CallbackRequest,
    ) -> Result<OAuthPollResponse> {
        let CallbackRequest { result, response } = callback_request;
        let code = match result {
            Ok(code) => code,
            Err(message) => {
                self.inner.oauth_sessions.lock().await.remove(session_id);
                let _ = response.send(CallbackOutcome {
                    success: false,
                    message: Some(message.clone()),
                });
                return Ok(OAuthPollResponse::Denied {
                    message: Some(message),
                });
            }
        };

        let result = async {
            let executable = self.executable()?;
            let entry = self.find_entry(&executable, &plugin_id).await?;
            let value = self
                .worker(&entry, &executable)
                .await
                .invoke(
                    "oauth.complete",
                    serde_json::json!({
                        "resourceType": resource_type,
                        "methodId": method_id,
                        "session": session,
                        "authorization": {
                            "code": code,
                            "redirectUri": redirect_uri,
                            "codeVerifier": code_verifier,
                        },
                    }),
                    CancellationToken::new(),
                )
                .await?;
            let resources: Vec<ResourceDraft> = serde_json::from_value(value)?;
            self.persist_oauth_resources(&entry, &executable, &plugin_id, &resource_type, resources)
                .await
        }
        .await;

        self.inner.oauth_sessions.lock().await.remove(session_id);
        match result {
            Ok(completed) => {
                let _ = response.send(CallbackOutcome {
                    success: true,
                    message: None,
                });
                Ok(completed)
            }
            Err(error) => {
                let message = error.to_string();
                let _ = response.send(CallbackOutcome {
                    success: false,
                    message: Some(message.clone()),
                });
                Ok(OAuthPollResponse::Failed { message })
            }
        }
    }

    async fn persist_oauth_resources(
        &self,
        entry: &PluginEntry,
        executable: &Path,
        plugin_id: &str,
        resource_type: &str,
        resources: Vec<ResourceDraft>,
    ) -> Result<OAuthPollResponse> {
        let outcome = self
            .persist_resources(entry, plugin_id, resource_type, resources)
            .await?;
        let model_sync_error = self
            .sync_provider_models_for_resource(entry, executable, resource_type)
            .await;
        Ok(OAuthPollResponse::Completed {
            added: outcome.added,
            updated: outcome.updated,
            model_sync_error,
        })
    }

    async fn persist_resources(
        &self,
        entry: &PluginEntry,
        plugin_id: &str,
        resource_type: &str,
        mut resources: Vec<ResourceDraft>,
    ) -> Result<super::state::UpsertOutcome> {
        if entry.manifest.permissions.resource_origin {
            for draft in &mut resources {
                let origin = origin::origin_from_private_data(&draft.private_data)?;
                origin::rewrite_private_origin(&mut draft.private_data, origin);
            }
        }
        let outcome = self
            .inner
            .state
            .upsert_resources(plugin_id, resource_type, resources)
            .await?;
        self.invalidate_live_catalogs(plugin_id, None).await;
        Ok(outcome)
    }

    pub async fn import_resources(
        &self,
        plugin_id: &str,
        resource_type: &str,
        files: serde_json::Value,
    ) -> Result<ImportResponse> {
        let executable = self.executable()?;
        let entry = self.find_entry(&executable, plugin_id).await?;
        let resource = find_resource(&entry, resource_type)?;
        if resource.import.is_none() {
            return Err(Error::Config(format!(
                "plugin '{plugin_id}' resource '{resource_type}' does not support import"
            )));
        }
        let value = self
            .worker(&entry, &executable)
            .await
            .invoke(
                "import.parse",
                serde_json::json!({ "resourceType": resource_type, "files": files }),
                CancellationToken::new(),
            )
            .await?;
        let parsed: ImportParseResult = serde_json::from_value(value)?;
        if parsed.resources.is_empty() {
            return Err(Error::Config(
                parsed
                    .warnings
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "import produced no resources".into()),
            ));
        }
        if parsed.resources.len() > MAX_IMPORT_DRAFTS {
            return Err(Error::Config(format!(
                "import produced more than {MAX_IMPORT_DRAFTS} resources"
            )));
        }
        let outcome = self
            .persist_resources(&entry, plugin_id, resource_type, parsed.resources)
            .await?;
        let model_sync_error = self
            .sync_provider_models_for_resource(&entry, &executable, resource_type)
            .await;
        Ok(ImportResponse {
            added: outcome.added,
            updated: outcome.updated,
            warnings: parsed.warnings,
            model_sync_error,
        })
    }

    /// 导出某资源类型的全部私有数据,供备份或迁移;格式与批量导入兼容。
    pub async fn export_resources(
        &self,
        plugin_id: &str,
        resource_type: &str,
    ) -> Result<serde_json::Value> {
        let executable = self.executable()?;
        let entry = self.find_entry(&executable, plugin_id).await?;
        find_resource(&entry, resource_type)?;
        let records = self.inner.state.resources(plugin_id, resource_type).await?;
        Ok(serde_json::json!({
            "accounts": records
                .iter()
                .map(|record| record.private_data.clone())
                .collect::<Vec<_>>(),
        }))
    }

    pub async fn refresh_resource(
        &self,
        plugin_id: &str,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<()> {
        let executable = self.executable()?;
        let entry = self.find_entry(&executable, plugin_id).await?;
        let resource = find_resource(&entry, resource_type)?;
        if !resource.can_refresh {
            return Err(Error::Config(format!(
                "plugin '{plugin_id}' resource '{resource_type}' does not support refresh"
            )));
        }
        let record = self
            .find_record(plugin_id, resource_type, resource_id)
            .await?;
        let value = self
            .worker(&entry, &executable)
            .await
            .invoke_with_origins(
                "resource.refresh",
                serde_json::json!({
                    "resourceType": resource_type,
                    "resource": record.snapshot(resource_type),
                }),
                CancellationToken::new(),
                self.resource_origins(&entry, Some(&record)),
            )
            .await?;
        let patch: ResourcePatch = serde_json::from_value(value)?;
        self.inner
            .state
            .apply_patch(plugin_id, resource_type, resource_id, patch)
            .await?;
        self.invalidate_live_catalogs(plugin_id, Some(resource_id))
            .await;
        Ok(())
    }

    pub async fn resource_action(
        &self,
        plugin_id: &str,
        resource_type: &str,
        resource_id: &str,
        action_id: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let executable = self.executable()?;
        let entry = self.find_entry(&executable, plugin_id).await?;
        let resource = find_resource(&entry, resource_type)?;
        let action = resource
            .actions
            .iter()
            .find(|action| action.id == action_id)
            .ok_or_else(|| {
                Error::Config(format!(
                    "plugin '{plugin_id}' resource '{resource_type}' does not define action '{action_id}'"
                ))
            })?;
        if !matches!(action.target.as_str(), "resource" | "card") {
            return Err(Error::Config(format!(
                "plugin '{plugin_id}' resource action '{action_id}' has an invalid target"
            )));
        }
        let record = self
            .find_record(plugin_id, resource_type, resource_id)
            .await?;
        let value = self
            .worker(&entry, &executable)
            .await
            .invoke_with_origins(
                "resource.action",
                serde_json::json!({
                    "resourceType": resource_type,
                    "actionId": action_id,
                    "resource": record.snapshot(resource_type),
                    "input": input,
                }),
                CancellationToken::new(),
                self.resource_origins(&entry, Some(&record)),
            )
            .await?;
        let result: ResourceActionResult = serde_json::from_value(value)?;
        if let Some(patch) = result.patch.clone() {
            self.inner
                .state
                .apply_patch(plugin_id, resource_type, resource_id, patch)
                .await?;
            self.invalidate_live_catalogs(plugin_id, Some(resource_id))
                .await;
        }
        Ok(serde_json::to_value(ResourceActionResponse::from(result))?)
    }

    pub async fn delete_resource(
        &self,
        plugin_id: &str,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<()> {
        let executable = self.executable()?;
        let entry = self.find_entry(&executable, plugin_id).await?;
        let resource = find_resource(&entry, resource_type)?;
        let record = self
            .find_record(plugin_id, resource_type, resource_id)
            .await?;
        if resource.can_remove {
            // 上游撤销失败不阻塞本地删除:用户必须能移除已失效的资源。
            if let Err(error) = self
                .worker(&entry, &executable)
                .await
                .invoke_with_origins(
                    "resource.remove",
                    serde_json::json!({
                        "resourceType": resource_type,
                        "resource": record.snapshot(resource_type),
                    }),
                    CancellationToken::new(),
                    self.resource_origins(&entry, Some(&record)),
                )
                .await
            {
                tracing::warn!(plugin = %plugin_id, %error, "plugin resource remove hook failed");
            }
        }
        self.inner
            .state
            .remove_resource(plugin_id, resource_type, resource_id)
            .await?;
        self.invalidate_live_catalogs(plugin_id, Some(resource_id))
            .await;
        Ok(())
    }

    pub async fn sync_models(&self, plugin_id: &str, provider_id: &str) -> Result<usize> {
        let executable = self.executable()?;
        let entry = self.find_entry(&executable, plugin_id).await?;
        let provider = find_provider(&entry, provider_id)?.clone();
        self.sync_provider_models(&entry, &executable, &provider)
            .await
    }

    pub async fn set_model_enabled(
        &self,
        plugin_id: &str,
        provider_id: &str,
        model_id: &str,
        enabled: bool,
    ) -> Result<()> {
        let executable = self.executable()?;
        let entry = self.find_entry(&executable, plugin_id).await?;
        let provider = find_provider(&entry, provider_id)?;
        if !provider.has_models {
            return Err(Error::Config(format!(
                "plugin provider '{provider_id}' does not enumerate models"
            )));
        }
        if provider.catalog == LIVE_CATALOG {
            return Err(Error::Config(
                "live plugin catalogs do not support per-model enablement".into(),
            ));
        }
        self.inner
            .state
            .set_model_enabled(plugin_id, provider_id, model_id, enabled)
            .await
    }

    pub async fn remove(&self, plugin_id: &str) -> Result<()> {
        if let Some(worker) = self.inner.workers.lock().await.remove(plugin_id) {
            worker.stop().await;
        }
        self.invalidate_live_catalogs(plugin_id, None).await;
        self.inner.state.clear(plugin_id).await
    }

    async fn descriptor(&self, entry: &PluginEntry, executable: &Path) -> PluginDescriptor {
        let plugin_id = &entry.manifest.id;
        let mut providers = Vec::new();
        for provider in &entry.definition.providers {
            let stored = self
                .provider_models(entry, executable, provider, false)
                .await;
            let configured = self.provider_configured(entry, provider).await;
            providers.push(PluginProviderDescriptor {
                id: provider.id.clone(),
                plugin_id: plugin_id.clone(),
                display_name: provider.display_name.clone(),
                description: provider.description.clone(),
                provider_type: provider.provider_type.clone(),
                resource_type: provider.resource_type.clone(),
                has_models: provider.has_models,
                catalog: provider.catalog.clone(),
                configured,
                models: stored
                    .iter()
                    .map(|model| {
                        PluginModelDescriptor::new(
                            plugin_id,
                            &entry.manifest.name,
                            &entry.icon,
                            provider,
                            model,
                        )
                    })
                    .collect(),
            });
        }
        let mut resources = Vec::new();
        for definition in &entry.definition.resources {
            let records = self
                .inner
                .state
                .resources(plugin_id, &definition.resource_type)
                .await
                .unwrap_or_default();
            let views = self
                .present_resources(entry, executable, definition, &records)
                .await;
            resources.push(PluginResourceDescriptor {
                resource_type: definition.resource_type.clone(),
                display_name: definition.display_name.clone(),
                add: definition.add.clone(),
                import: definition.import.clone(),
                actions: definition.actions.clone(),
                can_refresh: definition.can_refresh,
                can_remove: definition.can_remove,
                resources: views,
            });
        }
        PluginDescriptor {
            id: plugin_id.clone(),
            name: entry.manifest.name.clone(),
            version: entry.manifest.version.clone(),
            author: entry.manifest.author.clone(),
            icon: entry.icon.clone(),
            providers,
            resources,
        }
    }

    async fn present_resources(
        &self,
        entry: &PluginEntry,
        executable: &Path,
        definition: &ResourceDefinition,
        records: &[ResourceRecord],
    ) -> Vec<PluginResourceView> {
        if records.is_empty() {
            return Vec::new();
        }
        let snapshots = records
            .iter()
            .map(|record| record.snapshot(&definition.resource_type))
            .collect::<Vec<_>>();
        let origins = records
            .iter()
            .flat_map(|record| self.resource_origins(entry, Some(record)))
            .collect();
        let presented = self
            .worker(entry, executable)
            .await
            .invoke_with_origins(
                "resource.present",
                serde_json::json!({
                    "resourceType": definition.resource_type,
                    "resources": snapshots,
                }),
                CancellationToken::new(),
                origins,
            )
            .await
            .and_then(|value| {
                serde_json::from_value::<Vec<ResourcePresentation>>(value).map_err(Error::from)
            });
        match presented {
            Ok(views) if views.len() == records.len() => records
                .iter()
                .zip(views)
                .map(|(record, view)| PluginResourceView::from_record(record, view))
                .collect(),
            Ok(_) | Err(_) => records
                .iter()
                .map(|record| {
                    PluginResourceView::from_record(
                        record,
                        ResourcePresentation {
                            display_name: record.key.clone(),
                            description: serde_json::Value::Null,
                            metrics: Vec::new(),
                        },
                    )
                })
                .collect(),
        }
    }

    async fn provider_configured(
        &self,
        entry: &PluginEntry,
        provider: &ProviderDefinition,
    ) -> bool {
        let plugin_id = &entry.manifest.id;
        if let Some(resource_type) = &provider.resource_type {
            if self
                .inner
                .state
                .resources(plugin_id, resource_type)
                .await
                .unwrap_or_default()
                .is_empty()
            {
                return false;
            }
        }
        if provider.has_models && provider.catalog == SNAPSHOT_CATALOG {
            let models = self
                .inner
                .state
                .models(plugin_id, &provider.id)
                .await
                .unwrap_or_default();
            if models.is_empty() {
                return false;
            }
        }
        true
    }

    /// 资源到位后刷新使用该资源类型的 Provider 模型目录;失败只报告不中断。
    async fn sync_provider_models_for_resource(
        &self,
        entry: &PluginEntry,
        executable: &Path,
        resource_type: &str,
    ) -> Option<String> {
        let mut errors = Vec::new();
        for provider in entry.definition.providers.clone() {
            if provider.resource_type.as_deref() != Some(resource_type) || !provider.has_models {
                continue;
            }
            if let Err(error) = self
                .sync_provider_models(entry, executable, &provider)
                .await
            {
                errors.push(format!("{}: {error}", provider.id));
            }
        }
        (!errors.is_empty()).then(|| errors.join("; "))
    }

    async fn sync_provider_models(
        &self,
        entry: &PluginEntry,
        executable: &Path,
        provider: &ProviderDefinition,
    ) -> Result<usize> {
        if !provider.has_models {
            return Err(Error::Config(format!(
                "plugin provider '{}' does not enumerate models",
                provider.id
            )));
        }
        if provider.catalog == LIVE_CATALOG {
            return self
                .refresh_live_provider_models(entry, executable, provider)
                .await;
        }
        let plugin_id = &entry.manifest.id;
        let resource = match &provider.resource_type {
            Some(resource_type) => {
                let record = self.select_resource(plugin_id, resource_type).await?;
                Some(record)
            }
            None => None,
        };
        let models = self
            .list_models(entry, executable, provider, resource.as_ref(), true)
            .await?;
        self.inner
            .state
            .replace_models(plugin_id, &provider.id, &models)
            .await?;
        Ok(models.len())
    }

    async fn refresh_live_provider_models(
        &self,
        entry: &PluginEntry,
        executable: &Path,
        provider: &ProviderDefinition,
    ) -> Result<usize> {
        let resource_type = provider.resource_type.as_deref().ok_or_else(|| {
            Error::Config(format!(
                "plugin provider '{}' live catalog requires a resource type",
                provider.id
            ))
        })?;
        let plugin_id = &entry.manifest.id;
        let records = self.inner.state.resources(plugin_id, resource_type).await?;
        if records.is_empty() {
            return Err(Error::Provider(format!(
                "plugin '{plugin_id}' has no '{resource_type}' resource; add one first"
            )));
        }
        let mut count = 0;
        let mut errors = Vec::new();
        for record in records {
            match self
                .live_models_for_resource(entry, executable, provider, &record, true)
                .await
            {
                Ok(models) => count += models.len(),
                Err(error) => errors.push(format!("{}: {error}", record.id)),
            }
        }
        if count == 0 && !errors.is_empty() {
            return Err(Error::Provider(errors.join("; ")));
        }
        Ok(count)
    }

    async fn provider_models(
        &self,
        entry: &PluginEntry,
        executable: &Path,
        provider: &ProviderDefinition,
        force: bool,
    ) -> Vec<StoredModel> {
        if !provider.has_models {
            return Vec::new();
        }
        if provider.catalog == LIVE_CATALOG {
            return self
                .live_provider_models(entry, executable, provider, force)
                .await;
        }
        self.inner
            .state
            .models(&entry.manifest.id, &provider.id)
            .await
            .unwrap_or_default()
    }

    async fn live_provider_models(
        &self,
        entry: &PluginEntry,
        executable: &Path,
        provider: &ProviderDefinition,
        force: bool,
    ) -> Vec<StoredModel> {
        let Some(resource_type) = provider.resource_type.as_deref() else {
            return Vec::new();
        };
        let records = self
            .inner
            .state
            .resources(&entry.manifest.id, resource_type)
            .await
            .unwrap_or_default();
        let mut models = Vec::new();
        for record in records {
            match self
                .live_models_for_resource(entry, executable, provider, &record, force)
                .await
            {
                Ok(resource_models) => models.extend(resource_models),
                Err(error) => {
                    tracing::warn!(
                        plugin = %entry.manifest.id,
                        provider = %provider.id,
                        resource = %record.id,
                        %error,
                        "live plugin catalog refresh failed"
                    );
                }
            }
        }
        models
    }

    async fn live_models_for_resource(
        &self,
        entry: &PluginEntry,
        executable: &Path,
        provider: &ProviderDefinition,
        record: &ResourceRecord,
        force: bool,
    ) -> Result<Vec<StoredModel>> {
        let key = LiveCatalogKey {
            plugin_id: entry.manifest.id.clone(),
            provider_id: provider.id.clone(),
            resource_id: record.id.clone(),
        };
        if !force {
            let catalogs = self.inner.live_catalogs.lock().await;
            if let Some(cached) = catalogs.get(&key) {
                if cached.fetched_at.elapsed() < LIVE_CATALOG_TTL {
                    return match &cached.error {
                        Some(error) if cached.models.is_empty() => {
                            Err(Error::Provider(error.clone()))
                        }
                        _ => Ok(cached.models.clone()),
                    };
                }
            }
        }
        match self
            .list_models(entry, executable, provider, Some(record), false)
            .await
        {
            Ok(models) => {
                let stored = models
                    .into_iter()
                    .map(|model| bind_live_model(model, &record.id))
                    .collect::<Vec<_>>();
                let mut catalogs = self.inner.live_catalogs.lock().await;
                catalogs.insert(
                    key,
                    LiveCatalogEntry {
                        models: stored.clone(),
                        fetched_at: Instant::now(),
                        error: None,
                    },
                );
                Ok(stored)
            }
            Err(error) => {
                let mut catalogs = self.inner.live_catalogs.lock().await;
                if let Some(cached) = catalogs.get_mut(&key) {
                    cached.fetched_at = Instant::now();
                    cached.error = Some(error.to_string());
                    return Ok(cached.models.clone());
                }
                catalogs.insert(
                    key,
                    LiveCatalogEntry {
                        models: Vec::new(),
                        fetched_at: Instant::now(),
                        error: Some(error.to_string()),
                    },
                );
                Err(error)
            }
        }
    }

    async fn list_models(
        &self,
        entry: &PluginEntry,
        executable: &Path,
        provider: &ProviderDefinition,
        resource: Option<&ResourceRecord>,
        reject_empty: bool,
    ) -> Result<Vec<StoredModel>> {
        let snapshot = resource
            .map(|record| record.snapshot(provider.resource_type.as_deref().unwrap_or_default()));
        let value = self
            .worker(entry, executable)
            .await
            .invoke_with_origins(
                "models.list",
                serde_json::json!({ "providerId": provider.id, "resource": snapshot }),
                CancellationToken::new(),
                self.resource_origins(entry, resource),
            )
            .await?;
        let definitions = value
            .as_array()
            .ok_or_else(|| Error::Protocol("plugin models.list must return an array".into()))?;
        let mut models = Vec::with_capacity(definitions.len());
        let mut seen = HashSet::new();
        for definition in definitions {
            let model = StoredModel::from_definition(definition)?;
            if seen.insert(model.id.clone()) {
                models.push(model);
            }
        }
        if reject_empty && models.is_empty() {
            return Err(Error::Provider(format!(
                "plugin provider '{}' returned no models",
                provider.id
            )));
        }
        Ok(models)
    }

    fn resource_origins(
        &self,
        entry: &PluginEntry,
        record: Option<&ResourceRecord>,
    ) -> HashSet<String> {
        if !entry.manifest.permissions.resource_origin {
            return HashSet::new();
        }
        record
            .and_then(|record| origin::origin_from_private_data(&record.private_data).ok())
            .into_iter()
            .collect()
    }

    async fn invalidate_live_catalogs(&self, plugin_id: &str, resource_id: Option<&str>) {
        self.inner.live_catalogs.lock().await.retain(|key, _| {
            key.plugin_id != plugin_id
                || resource_id.is_some_and(|resource_id| key.resource_id != resource_id)
        });
    }

    /// 第一版选择策略:按创建顺序取首个可用资源;冷却到期视为可用。
    async fn select_resource(
        &self,
        plugin_id: &str,
        resource_type: &str,
    ) -> Result<ResourceRecord> {
        let records = self.inner.state.resources(plugin_id, resource_type).await?;
        if records.is_empty() {
            return Err(Error::Provider(format!(
                "plugin '{plugin_id}' has no '{resource_type}' resource; add one first"
            )));
        }
        let now = now_ms();
        records
            .iter()
            .find(|record| record.state.is_ready(now))
            .or_else(|| records.first())
            .cloned()
            .ok_or_else(|| Error::Provider("no plugin resource is available".into()))
    }

    async fn find_record(
        &self,
        plugin_id: &str,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<ResourceRecord> {
        self.inner
            .state
            .resources(plugin_id, resource_type)
            .await?
            .into_iter()
            .find(|record| record.id == resource_id)
            .ok_or_else(|| Error::RunNotFound(format!("plugin resource {resource_id}")))
    }

    async fn update_session(
        &self,
        session_id: &str,
        session: Option<serde_json::Value>,
        poll_interval_ms: Option<i64>,
    ) {
        let mut sessions = self.inner.oauth_sessions.lock().await;
        if let Some(state) = sessions.get_mut(session_id) {
            if let Some(session) = session {
                state.session = session;
            }
            if let Some(interval) = poll_interval_ms {
                state.poll_interval_ms = interval;
            }
        }
    }

    fn executable(&self) -> Result<std::path::PathBuf> {
        self.inner
            .runtime
            .executable()
            .ok_or_else(|| Error::Config("plugin runtime is not ready".into()))
    }

    async fn entries(&self, executable: &Path) -> Vec<PluginEntry> {
        if let Some(entries) = self.inner.entries.read().await.as_ref() {
            return entries.clone();
        }
        let loaded = self.inner.catalog.entries(executable).await;
        *self.inner.entries.write().await = Some(loaded.clone());
        loaded
    }

    async fn find_entry(&self, executable: &Path, plugin_id: &str) -> Result<PluginEntry> {
        self.entries(executable)
            .await
            .into_iter()
            .find(|entry| entry.manifest.id == plugin_id)
            .ok_or_else(|| Error::RunNotFound(format!("plugin {plugin_id}")))
    }

    async fn worker(&self, entry: &PluginEntry, executable: &Path) -> Arc<PluginWorker> {
        let mut workers = self.inner.workers.lock().await;
        workers
            .entry(entry.manifest.id.clone())
            .or_insert_with(|| {
                Arc::new(PluginWorker::new(
                    entry,
                    executable.to_path_buf(),
                    self.inner.catalog.loader().clone(),
                    self.inner.store.clone(),
                ))
            })
            .clone()
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OAuth2Begin {
    session: serde_json::Value,
    user_code: String,
    verification_url: String,
    #[serde(default)]
    verification_url_complete: Option<String>,
    expires_at_ms: i64,
    poll_interval_ms: i64,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OAuth2AuthorizationCodeBegin {
    session: serde_json::Value,
    authorization_url: String,
    expires_at_ms: i64,
    #[serde(default)]
    poll_interval_ms: Option<i64>,
}

fn oauth_random_secret() -> String {
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let mut bytes = [0_u8; 32];
    bytes[..16].copy_from_slice(first.as_bytes());
    bytes[16..].copy_from_slice(second.as_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case", tag = "status")]
enum OAuth2Poll {
    #[serde(rename_all = "camelCase")]
    Pending {
        #[serde(default)]
        session: Option<serde_json::Value>,
    },
    #[serde(rename_all = "camelCase")]
    SlowDown {
        #[serde(default)]
        session: Option<serde_json::Value>,
    },
    #[serde(rename_all = "camelCase")]
    Completed { resources: Vec<ResourceDraft> },
    #[serde(rename_all = "camelCase")]
    Denied {
        #[serde(default)]
        message: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Failed { message: String },
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportParseResult {
    resources: Vec<ResourceDraft>,
    #[serde(default)]
    warnings: Vec<String>,
}

fn find_provider<'a>(entry: &'a PluginEntry, provider_id: &str) -> Result<&'a ProviderDefinition> {
    entry
        .definition
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| {
            Error::RunNotFound(format!(
                "plugin '{}' provider {provider_id}",
                entry.manifest.id
            ))
        })
}

fn find_resource<'a>(
    entry: &'a PluginEntry,
    resource_type: &str,
) -> Result<&'a ResourceDefinition> {
    entry
        .definition
        .resources
        .iter()
        .find(|resource| resource.resource_type == resource_type)
        .ok_or_else(|| {
            Error::RunNotFound(format!(
                "plugin '{}' resource type {resource_type}",
                entry.manifest.id
            ))
        })
}

fn bind_live_model(mut model: StoredModel, resource_id: &str) -> StoredModel {
    model.id = format!("{resource_id}/{}", model.id);
    model
}

fn live_resource_id(stored_id: &str) -> Option<&str> {
    stored_id
        .split_once('/')
        .map(|(resource_id, _)| resource_id)
}

fn live_upstream_id(stored_id: &str) -> Option<&str> {
    stored_id.split_once('/').map(|(_, upstream)| upstream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_live_models_to_resource_ids() {
        let model = bind_live_model(
            StoredModel {
                id: "gpt-5".into(),
                display_name: "GPT".into(),
                description: None,
                max_output_tokens: None,
                images: false,
                enabled: true,
                private_data: serde_json::Value::Null,
            },
            "resource-1",
        );
        assert_eq!(model.id, "resource-1/gpt-5");
        assert_eq!(live_resource_id(&model.id), Some("resource-1"));
        assert_eq!(live_upstream_id(&model.id), Some("gpt-5"));
        assert_eq!(live_upstream_id("resource-1/org/gpt-5"), Some("org/gpt-5"));
    }
}
