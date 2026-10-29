// app.js 管理协议调试器列表筛选、详情编辑器和实时事件交互。
import { getLocale, setLocale, t, translateDocument } from "./i18n.js";
import { bindEvents, renderPauseState } from "./app_events.js";
import { escapeHTML, formatBytes, formatDuration, formatHex, formatState, renderDecodeError, renderTruncated } from "./view_helpers.js";
const monacoReady = loadMonaco();
const editorSlots = {
  request: { editor: null, model: null, host: null, token: 0, value: "", language: "plaintext" },
  response: { editor: null, model: null, host: null, token: 0, value: "", language: "plaintext" },
};

const state = {
  status: null,
  exchanges: [],
  conversations: [],
  selectedId: null,
  selected: null,
  search: "",
  requestId: "",
  conversationId: "",
  endpoint: "all",
  bidiMessageKinds: new Set(),
  showOptions: false,
  sortOrder: "desc",
  paused: false,
  pendingRefresh: false,
  connection: { connected: false, key: "status.connecting", values: {} },
  tabs: {
    request: "body",
    response: "body",
  },
};

const elements = {
  statusDot: document.querySelector("#status-dot"),
  statusText: document.querySelector("#status-text"),
  serviceAddress: document.querySelector("#service-address"),
  upstreamURL: document.querySelector("#upstream-url"),
  connectionLabel: document.querySelector("#connection-label"),
  trafficSummary: document.querySelector("#traffic-summary"),
  searchInput: document.querySelector("#search-input"),
  requestIdInput: document.querySelector("#request-id-input"),
  conversationSelect: document.querySelector("#conversation-select"),
  endpointFilter: document.querySelector("#endpoint-filter"),
  bidiMessageFilter: document.querySelector("#bidi-message-filter"),
  bidiMessageOptions: document.querySelector("#bidi-message-options"),
  showOptionsCheckbox: document.querySelector("#show-options-checkbox"),
  sortOrder: document.querySelector("#sort-order"),
  requestCount: document.querySelector("#request-count"),
  requestList: document.querySelector("#request-list"),
  emptyState: document.querySelector("#empty-state"),
  selectionSummary: document.querySelector("#selection-summary"),
  requestContent: document.querySelector("#request-content"),
  responseContent: document.querySelector("#response-content"),
  pauseButton: document.querySelector("#pause-button"),
  clearButton: document.querySelector("#clear-button"),
  localeSelect: document.querySelector("#locale-select"),
  workspace: document.querySelector("#workspace"),
  splitter: document.querySelector("#horizontal-splitter"),
};

async function fetchJSON(url, options) {
  const response = await fetch(url, options);
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  if (response.status === 204) return null;
  return response.json();
}

async function loadStatus() {
  state.status = await fetchJSON("api/status");
  elements.statusDot.classList.toggle("online", Boolean(state.status.running));
  renderRuntimeStatus();
  elements.serviceAddress.textContent = `http://${state.status.serviceAddr}`;
  elements.upstreamURL.textContent = state.status.upstreamURL;
  elements.showOptionsCheckbox.checked = state.showOptions;
}

async function refreshList() {
  const query = state.conversationId ? `?conversation_id=${encodeURIComponent(state.conversationId)}` : "";
  [state.exchanges, state.conversations] = await Promise.all([
    fetchJSON(`api/exchanges${query}`),
    fetchJSON("api/conversations"),
  ]);
  renderConversationOptions();
  renderBidiMessageFilter();
  renderList();
  renderTrafficSummary();
  if (state.selectedId && state.exchanges.some((item) => item.id === state.selectedId)) {
    await refreshDetail(state.selectedId);
  } else if (state.selectedId) {
    state.selectedId = null;
    state.selected = null;
    renderDetail();
  }
}

function renderConversationOptions() {
  const selected = state.conversationId;
  const options = [`<option value="">${escapeHTML(t("filters.allConversations"))}</option>`];
  for (const conversation of state.conversations) {
    if (!conversation.conversationId) continue;
    const label = `${conversation.conversationId} (${conversation.exchangeCount})`;
    options.push(`<option value="${escapeHTML(conversation.conversationId)}">${escapeHTML(label)}</option>`);
  }
  elements.conversationSelect.innerHTML = options.join("");
  elements.conversationSelect.value = selected;
}

async function refreshDetail(id) {
  if (!id) return;
  try {
    const detail = await fetchJSON(`api/exchanges/${encodeURIComponent(id)}`);
    if (state.selectedId !== id) return;
    state.selected = detail;
    renderDetail();
  } catch (error) {
    if (state.selectedId === id) {
      state.selected = null;
      renderDetailError(error);
    }
  }
}

function scheduleRefresh() {
  if (state.paused) {
    state.pendingRefresh = true;
    return;
  }
  if (state.pendingRefresh) return;
  state.pendingRefresh = true;
  window.setTimeout(async () => {
    state.pendingRefresh = false;
    try {
      await refreshList();
    } catch (error) {
      setConnectionState(false, "connection.refreshFailed", { message: error.message });
    }
  }, 90);
}

function connectEvents() {
  const events = new EventSource("api/events");
  events.addEventListener("open", () => setConnectionState(true, "connection.live"));
  events.addEventListener("update", scheduleRefresh);
  events.addEventListener("error", () => setConnectionState(false, "connection.retrying"));
}

function setConnectionState(connected, key, values = {}) {
  state.connection = { connected, key, values };
  renderConnectionState();
}

function renderRuntimeStatus() {
  if (!state.status) {
    elements.statusText.textContent = t("status.connecting");
    return;
  }
  elements.statusText.textContent = t(state.status.running ? "status.running" : "status.stopped");
}

function renderConnectionState() {
  const { connected, key, values } = state.connection;
  elements.connectionLabel.textContent = t(key, values);
  elements.statusDot.classList.toggle("online", connected && Boolean(state.status?.running));
}

function filteredExchanges() {
  const query = state.search.trim().toLowerCase();
  const requestId = state.requestId.trim().toLowerCase();
  const direction = state.sortOrder === "asc" ? 1 : -1;
  return state.exchanges
    .filter((item) => {
      if (!state.showOptions && String(item.method || "").toUpperCase() === "OPTIONS") return false;
      if (state.endpoint === "runsse" && !item.path.toLowerCase().includes("runsse")) return false;
      if (state.endpoint === "bidiappend" && !item.path.toLowerCase().includes("bidiappend")) return false;
      if (state.endpoint === "bidiappend" && state.bidiMessageKinds.size > 0 && !state.bidiMessageKinds.has(item.requestKind || "")) return false;
      if (requestId && !String(item.requestId || "").toLowerCase().includes(requestId)) return false;
      if (!query) return true;
      return [item.url, item.requestId, item.requestKind, item.responseKind, item.state, String(item.status)]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(query));
    })
    .sort((left, right) => {
      const startedAtDelta = new Date(left.startedAt).getTime() - new Date(right.startedAt).getTime();
      if (startedAtDelta !== 0) return startedAtDelta * direction;
      return left.id.localeCompare(right.id, undefined, { numeric: true }) * direction;
    });
}

function renderBidiMessageFilter() {
  const visible = state.endpoint === "bidiappend";
  elements.bidiMessageFilter.hidden = !visible;
  if (!visible) elements.bidiMessageFilter.open = false;

  const availableKinds = state.exchanges
    .filter((item) => item.path.toLowerCase().includes("bidiappend") && item.requestKind)
    .map((item) => item.requestKind);
  const kinds = [...new Set([...availableKinds, ...state.bidiMessageKinds])].sort((left, right) => left.localeCompare(right));
  elements.bidiMessageFilter.querySelector("summary").textContent = state.bidiMessageKinds.size
    ? t("filters.selectedMessageTypes", { count: state.bidiMessageKinds.size })
    : t("filters.allMessageTypes");
  elements.bidiMessageOptions.innerHTML = [
    `<label class="multi-select-option all-option"><input type="checkbox" value=""${state.bidiMessageKinds.size === 0 ? " checked" : ""}><span>${escapeHTML(t("filters.allMessageTypes"))}</span></label>`,
    ...kinds.map((kind) => `<label class="multi-select-option"><input type="checkbox" value="${escapeHTML(kind)}"${state.bidiMessageKinds.has(kind) ? " checked" : ""}><span title="${escapeHTML(kind)}">${escapeHTML(kind)}</span></label>`),
  ].join("");
}

function renderList() {
  const exchanges = filteredExchanges();
  elements.requestCount.textContent = t("count.requests", { count: exchanges.length });
  elements.emptyState.classList.toggle("hidden", exchanges.length > 0);
  const groups = new Map();
  for (const item of exchanges) {
    const conversationID = item.conversationId || "";
    if (!groups.has(conversationID)) groups.set(conversationID, []);
    groups.get(conversationID).push(item);
  }
  elements.requestList.innerHTML = [...groups.entries()]
    .map(([conversationID, items]) => {
      const label = conversationID || t("groups.unassigned");
      const header = `<tr class="conversation-group"><td colspan="9"><span>${escapeHTML(t("groups.conversation"))}</span><code title="${escapeHTML(label)}">${escapeHTML(label)}</code><strong>${items.length}</strong></td></tr>`;
      const rows = items.map((item) => {
      const selected = item.id === state.selectedId ? " selected" : "";
      const statusClass = item.status >= 400 ? "error" : item.status ? "success" : "";
      const kind = item.responseKind || item.requestKind || "-";
      return `<tr class="${selected.trim()}" data-id="${escapeHTML(item.id)}">
        <td><span class="row-state ${escapeHTML(item.state)}"></span></td>
        <td><code>${escapeHTML(item.id)}</code></td>
        <td title="${escapeHTML(item.url)}"><code>${escapeHTML(item.url)}</code></td>
        <td title="${escapeHTML(item.requestId || "")}"><code class="request-id-text">${escapeHTML(item.requestId || "-")}</code></td>
        <td><span class="kind-text">${escapeHTML(kind)}</span></td>
        <td><span class="method-text">${escapeHTML(item.method)}</span></td>
        <td><span class="status-text ${statusClass}">${item.status || "-"}</span></td>
        <td>${formatBytes(item.responseBytes)}</td>
        <td>${formatDuration(item.durationMs)}</td>
      </tr>`;
      }).join("");
      return header + rows;
    })
    .join("");
}

function renderTrafficSummary() {
  const totals = state.exchanges.reduce(
    (result, item) => {
      result.up += item.requestBytes || 0;
      result.down += item.responseBytes || 0;
      return result;
    },
    { up: 0, down: 0 },
  );
  elements.trafficSummary.textContent = `↑ ${formatBytes(totals.up)}　↓ ${formatBytes(totals.down)}`;
}

function renderDetail() {
  if (!state.selected) {
    disposeEditor("request");
    disposeEditor("response");
    elements.selectionSummary.innerHTML = `<span class="method-badge">POST</span><span class="status-badge">${escapeHTML(t("selection.waiting"))}</span><code>${escapeHTML(t("selection.prompt"))}</code>`;
    elements.requestContent.classList.remove("editor-active");
    elements.responseContent.classList.remove("editor-active");
    elements.requestContent.innerHTML = `<div class="notice">${escapeHTML(t("notices.noRequest"))}</div>`;
    elements.responseContent.innerHTML = `<div class="notice">${escapeHTML(t("notices.noResponse"))}</div>`;
    return;
  }
  const item = state.selected;
  const statusClass = item.status >= 200 && item.status < 400 ? "success" : "";
  elements.selectionSummary.innerHTML = `<span class="method-badge">${escapeHTML(item.method)}</span><span class="status-badge ${statusClass}">${escapeHTML(item.status || formatState(item.state))}</span><code>${escapeHTML(item.url)}</code>`;
  renderPayload("request", item.request, state.tabs.request);
  renderPayload("response", item.response, state.tabs.response);
}

function renderDetailError(error) {
  disposeEditor("request");
  disposeEditor("response");
  elements.requestContent.classList.remove("editor-active");
  elements.responseContent.classList.remove("editor-active");
  elements.requestContent.innerHTML = `<div class="notice error">${escapeHTML(error.message)}</div>`;
  elements.responseContent.innerHTML = `<div class="notice error">${escapeHTML(error.message)}</div>`;
}

function renderPayload(side, payload, tab) {
  const container = elements[`${side}Content`];
  if (!payload) {
    renderStaticPayload(side, `<div class="notice">${escapeHTML(t("notices.noContent"))}</div>`);
    return;
  }
  if (tab === "headers") {
    renderStaticPayload(side, renderHeaders(payload.headers));
    return;
  }
  if (tab === "raw") {
    if (!payload.rawHex) {
      renderStaticPayload(side, `<div class="notice">${escapeHTML(t("notices.noRaw"))}</div>`);
      return;
    }
    renderEditorPayload(side, formatHex(payload.rawHex), "plaintext", renderTruncated(payload.rawTruncated));
    return;
  }
  if (tab === "frames") {
    const document = frameEditorDocument(payload.frames);
    if (!document) {
      renderStaticPayload(side, `<div class="notice">${escapeHTML(t("notices.noFrames"))}</div>`);
      return;
    }
    renderEditorPayload(side, document, "json", "");
    return;
  }
  if (payload.decodedJson) {
    renderEditorPayload(side, payload.decodedJson, payload.decodedLanguage || "json", `${renderDecodeError(payload.decodeError)}${renderTruncated(payload.rawTruncated)}`);
    return;
  }
  if (payload.frames?.length) {
    renderEditorPayload(side, frameEditorDocument(payload.frames), "json", "");
    return;
  }
  if (payload.decodeError) {
    renderStaticPayload(side, `<div class="notice error">${escapeHTML(payload.decodeError)}</div>`);
    return;
  }
  container.classList.remove("editor-active");
  renderStaticPayload(side, `<div class="notice">${escapeHTML(t("notices.noBody"))}</div>`);
}

function renderHeaders(headers = []) {
  const items = Array.isArray(headers) ? headers : [];
  if (!items.length) return `<div class="notice">${escapeHTML(t("notices.noHeaders"))}</div>`;
  return `<table class="headers-table"><tbody>${items
    .map((header) => `<tr><th>${escapeHTML(header.name)}</th><td>${escapeHTML(header.value)}</td></tr>`)
    .join("")}</tbody></table>`;
}

function frameEditorDocument(frames = []) {
  const items = Array.isArray(frames) ? frames : [];
  if (!items.length) return "";
  const normalized = items.map((frame) => {
    let message = frame.rawHex || null;
    if (frame.json) {
      try {
        message = JSON.parse(frame.json);
      } catch {
        message = frame.json;
      }
    }
    return {
      index: frame.index,
      kind: frame.kind || frame.messageType || t("notices.unknown"),
      messageType: frame.messageType || undefined,
      flags: `0x${Number(frame.flags || 0).toString(16).padStart(2, "0")}`,
      length: frame.length,
      compressed: Boolean(frame.compressed),
      endStream: Boolean(frame.endStream),
      requestId: frame.requestId || undefined,
      error: frame.error || undefined,
      message,
    };
  });
  return JSON.stringify(normalized, null, 2);
}

function renderStaticPayload(side, markup) {
  disposeEditor(side);
  const container = elements[`${side}Content`];
  container.classList.remove("editor-active");
  container.innerHTML = markup;
}

function renderEditorPayload(side, value, language, notices) {
  const container = elements[`${side}Content`];
  const slot = editorSlots[side];
  slot.value = value;
  slot.language = language;
  container.classList.add("editor-active");
  let host = container.querySelector(".editor-host");
  if (!host || slot.host !== host) {
    disposeEditor(side);
    slot.value = value;
    slot.language = language;
    container.innerHTML = `<div class="editor-host"><pre class="editor-fallback">${escapeHTML(value)}</pre></div><div class="editor-notices">${notices}</div>`;
    host = container.querySelector(".editor-host");
    void createEditor(side, host, value, language);
    return;
  }
  container.querySelector(".editor-notices").innerHTML = notices;
  const fallback = host.querySelector(".editor-fallback");
  if (fallback) fallback.textContent = value;
  updateEditor(slot, value, language);
}

async function createEditor(side, host, value, language) {
  const slot = editorSlots[side];
  const token = ++slot.token;
  slot.host = host;
  try {
    const monaco = await monacoReady;
    if (token !== slot.token || !host.isConnected) return;
    host.textContent = "";
    const model = monaco.editor.createModel(slot.value || value, slot.language || language);
    const editor = monaco.editor.create(host, {
      model,
      theme: "vs-dark",
      readOnly: true,
      domReadOnly: true,
      automaticLayout: true,
      fontFamily: "SFMono-Regular, Consolas, Liberation Mono, monospace",
      fontSize: 12,
      lineHeight: 19,
      minimap: { enabled: false },
      glyphMargin: false,
      folding: true,
      lineNumbersMinChars: 3,
      overviewRulerLanes: 0,
      overviewRulerBorder: false,
      renderLineHighlight: "none",
      scrollBeyondLastLine: false,
      smoothScrolling: true,
      wordWrap: "off",
      padding: { top: 8, bottom: 16 },
      stickyScroll: { enabled: false },
      contextmenu: true,
    });
    slot.editor = editor;
    slot.model = model;
    slot.host = host;
  } catch {
    // Monaco 初始化失败时保留文本回退视图。
  }
}

function updateEditor(slot, value, language) {
  if (!slot.editor || !slot.model) return;
  const monaco = window.monaco;
  if (monaco && slot.model.getLanguageId() !== language) monaco.editor.setModelLanguage(slot.model, language);
  if (slot.model.getValue() === value) return;
  const viewState = slot.editor.saveViewState();
  slot.model.setValue(value);
  if (viewState) slot.editor.restoreViewState(viewState);
}

function disposeEditor(side) {
  const slot = editorSlots[side];
  slot.token += 1;
  slot.editor?.dispose();
  slot.model?.dispose();
  slot.editor = null;
  slot.model = null;
  slot.host = null;
  slot.value = "";
  slot.language = "plaintext";
}

function loadMonaco() {
  return new Promise((resolve, reject) => {
    const amdRequire = window.require;
    if (typeof amdRequire !== "function" || typeof amdRequire.config !== "function") {
      reject(new Error("Monaco loader is unavailable"));
      return;
    }
    amdRequire.config({ paths: { vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.56.0/min/vs" } });
    amdRequire(["vs/editor/editor.main"], () => resolve(window.monaco), reject);
  });
}

function applyLocale() {
  translateDocument();
  elements.localeSelect.value = getLocale();
  renderRuntimeStatus();
  renderConnectionState();
  renderPauseState(state, elements);
  renderConversationOptions();
  renderBidiMessageFilter();
  renderList();
  renderTrafficSummary();
  renderDetail();
}
bindEvents({
  state,
  elements,
  fetchJSON,
  refreshList,
  refreshDetail,
  renderList,
  renderDetail,
  renderBidiMessageFilter,
  setConnectionState,
  applyLocale,
});
async function bootstrap() {
  applyLocale();
  renderDetail();
  try {
    await Promise.all([loadStatus(), refreshList()]);
    connectEvents();
  } catch (error) {
    setConnectionState(false, "connection.connectFailed", { message: error.message });
  }
}

void bootstrap();
