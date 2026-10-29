// view_helpers.js 提供调试器界面使用的格式化、转义和复制文本辅助函数。
import { t } from "./i18n.js";

// renderDecodeError 将解码错误转换为安全的提示片段。
export function renderDecodeError(error) {
  return error ? `<div class="frame-error">${escapeHTML(error)}</div>` : "";
}

// renderTruncated 生成正文被截断时的提示片段。
export function renderTruncated(truncated) {
  return truncated ? `<div class="truncated-notice">${escapeHTML(t("notices.truncated"))}</div>` : "";
}

// formatState 将捕获状态转换为当前语言的展示文本。
export function formatState(value) {
  const key = {
    pending: "state.pending",
    streaming: "state.streaming",
    completed: "state.completed",
    error: "state.error",
  }[value];
  return key ? t(key) : value || "-";
}

// currentCopyText 根据当前标签页提取可复制的载荷文本。
export function currentCopyText(side, state) {
  const payload = state.selected?.[side];
  if (!payload) return "";
  const tab = state.tabs[side];
  if (tab === "headers") return (payload.headers || []).map((item) => `${item.name}: ${item.value}`).join("\n");
  if (tab === "raw") return payload.rawHex || "";
  if (tab === "frames") return (payload.frames || []).map((frame) => frame.json || frame.rawHex || frame.error || "").join("\n\n");
  return payload.decodedJson || "";
}

// formatHex 将十六进制载荷按行格式化为调试视图。
export function formatHex(value) {
  const hex = String(value || "").replace(/[^0-9a-f]/gi, "");
  const lines = [];
  for (let index = 0; index < hex.length; index += 32) {
    const chunk = hex.slice(index, index + 32);
    const bytes = chunk.match(/.{1,2}/g) || [];
    lines.push(`${(index / 2).toString(16).padStart(8, "0")}  ${bytes.join(" ")}`);
  }
  return lines.join("\n");
}

// formatBytes 将字节数格式化为人类可读的单位。
export function formatBytes(value) {
  const bytes = Number(value || 0);
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

// formatDuration 将毫秒耗时格式化为人类可读的单位。
export function formatDuration(value) {
  const milliseconds = Number(value || 0);
  if (milliseconds < 1000) return `${milliseconds} ms`;
  return `${(milliseconds / 1000).toFixed(1)} s`;
}

// escapeHTML 转义用户或网络输入，避免插入界面时形成 HTML。
export function escapeHTML(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}
