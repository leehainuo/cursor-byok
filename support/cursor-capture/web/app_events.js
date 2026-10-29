// app_events.js 绑定调试器筛选、详情、暂停和布局交互事件。
import { t } from "./i18n.js";
import { currentCopyText } from "./view_helpers.js";

// renderPauseState 更新暂停按钮的文本和可访问性属性。
export function renderPauseState(state, elements) {
  elements.pauseButton.textContent = state.paused ? "▶" : "Ⅱ";
  const actionKey = state.paused ? "actions.resume" : "actions.pause";
  elements.pauseButton.title = t(actionKey);
  elements.pauseButton.setAttribute("aria-label", t(actionKey));
}

// bindEvents 绑定调试器页面的筛选、详情、暂停和布局交互。
export function bindEvents({ state, elements, fetchJSON, refreshList, refreshDetail, renderList, renderDetail, renderBidiMessageFilter, setConnectionState, applyLocale }) {
elements.requestList.addEventListener("click", async (event) => {
  const row = event.target.closest("tr[data-id]");
  if (!row) return;
  state.selectedId = row.dataset.id;
  state.selected = null;
  renderList();
  renderDetail();
  await refreshDetail(state.selectedId);
});

elements.searchInput.addEventListener("input", (event) => {
  state.search = event.target.value;
  renderList();
});

elements.requestIdInput.addEventListener("input", (event) => {
  state.requestId = event.target.value;
  renderList();
});

elements.conversationSelect.addEventListener("change", async (event) => {
  state.conversationId = event.target.value;
  state.selectedId = null;
  state.selected = null;
  await refreshList();
  renderDetail();
});

elements.endpointFilter.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-value]");
  if (!button) return;
  state.endpoint = button.dataset.value;
  for (const item of elements.endpointFilter.querySelectorAll("button")) {
    item.classList.toggle("active", item === button);
  }
  renderBidiMessageFilter();
  renderList();
});

elements.bidiMessageOptions.addEventListener("change", (event) => {
  const checkbox = event.target.closest('input[type="checkbox"]');
  if (!checkbox) return;
  if (!checkbox.value) {
    state.bidiMessageKinds.clear();
  } else if (checkbox.checked) {
    state.bidiMessageKinds.add(checkbox.value);
  } else {
    state.bidiMessageKinds.delete(checkbox.value);
  }
  renderBidiMessageFilter();
  elements.bidiMessageFilter.open = true;
  renderList();
});

document.addEventListener("click", (event) => {
  if (!elements.bidiMessageFilter.contains(event.target)) elements.bidiMessageFilter.open = false;
});

elements.sortOrder.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-value]");
  if (!button) return;
  state.sortOrder = button.dataset.value;
  for (const item of elements.sortOrder.querySelectorAll("button")) {
    item.classList.toggle("active", item === button);
  }
  renderList();
});

document.querySelectorAll(".payload-panel").forEach((panel) => {
  panel.querySelector(".tabs").addEventListener("click", (event) => {
    const button = event.target.closest("button[data-tab]");
    if (!button) return;
    const side = panel.dataset.side;
    state.tabs[side] = button.dataset.tab;
    panel.querySelectorAll(".tabs button").forEach((item) => item.classList.toggle("active", item === button));
    renderDetail();
  });
});

document.querySelectorAll("[data-copy-side]").forEach((button) => {
  button.addEventListener("click", async () => {
    const text = currentCopyText(button.dataset.copySide, state);
    if (!text) return;
    await navigator.clipboard.writeText(text);
    button.textContent = t("actions.copied");
    window.setTimeout(() => {
      button.textContent = t("actions.copy");
    }, 900);
  });
});

elements.pauseButton.addEventListener("click", async () => {
  state.paused = !state.paused;
  elements.pauseButton.classList.toggle("active", state.paused);
  renderPauseState(state, elements);
  setConnectionState(!state.paused, state.paused ? "connection.paused" : "connection.live");
  if (!state.paused && state.pendingRefresh) {
    state.pendingRefresh = false;
    await refreshList();
  }
});

elements.localeSelect.addEventListener("change", (event) => {
  setLocale(event.target.value);
  applyLocale();
});

elements.showOptionsCheckbox.addEventListener("change", (event) => {
  state.showOptions = event.target.checked;
  if (!state.showOptions && String(state.selected?.method || "").toUpperCase() === "OPTIONS") {
    state.selectedId = null;
    state.selected = null;
    renderDetail();
  }
  renderList();
});

elements.clearButton.addEventListener("click", async () => {
  await fetchJSON("api/exchanges", { method: "DELETE" });
  state.selectedId = null;
  state.selected = null;
  state.conversationId = "";
  await refreshList();
  renderDetail();
});

let draggingSplitter = false;
elements.splitter.addEventListener("pointerdown", (event) => {
  draggingSplitter = true;
  elements.splitter.classList.add("dragging");
  elements.splitter.setPointerCapture(event.pointerId);
});

elements.splitter.addEventListener("pointermove", (event) => {
  if (!draggingSplitter) return;
  const bounds = elements.workspace.getBoundingClientRect();
  const top = Math.max(180, Math.min(bounds.height - 225, event.clientY - bounds.top));
  elements.workspace.style.gridTemplateRows = `${top}px 5px minmax(220px, 1fr)`;
});

elements.splitter.addEventListener("pointerup", () => {
  draggingSplitter = false;
  elements.splitter.classList.remove("dragging");
});
}
