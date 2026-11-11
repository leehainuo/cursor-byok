import type { JsonValue, PluginContext } from "../plugin.ts";
import type { LlmContentPart, LlmRequest, ModelEvent, ProviderOutput } from "../provider.ts";

/** 本协议产生的回放状态种类;与宿主内置 Anthropic Provider 一致,可互相回放。 */
export const REPLAY_KIND = "anthropic";

const DEFAULT_MAX_TOKENS = 65_000;
const CACHE_CONTROL: JsonValue = { type: "ephemeral" };

/** 上游返回非 2xx 时抛出,携带完整响应体供调用方分类。 */
export class HttpError extends Error {
  constructor(readonly status: number, readonly body: string) {
    super(`HTTP ${status}: ${body}`);
  }
}

export type AnthropicCall = {
  url: string;
  model: string;
  request: LlmRequest;
  headers?: Record<string, string>;
  /** 最后合并进请求体。 */
  extraBody?: Record<string, JsonValue>;
};

type UsageState = {
  inputTokens: number | null;
  outputTokens: number | null;
  totalTokens: number | null;
  cacheReadTokens: number | null;
  cacheWriteTokens: number | null;
  reasoningTokens: number | null;
};

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

function text(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function count(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function requiredIndex(value: Record<string, unknown>): number {
  const index = count(value.index);
  if (index === null) throw new Error("Anthropic event is missing index");
  return index;
}

function requiredType(value: Record<string, unknown>): string {
  const kind = text(value.type);
  if (kind === null) throw new Error("Anthropic event is missing type");
  return kind;
}

function anthropicParts(parts: LlmContentPart[], allowImages: boolean): JsonValue[] {
  const content: JsonValue[] = [];
  for (const part of parts) {
    if (part.type === "text") {
      if (part.text) content.push({ type: "text", text: part.text });
      continue;
    }
    if (!allowImages) throw new Error("Anthropic only accepts images in user messages");
    content.push({
      type: "image",
      source: { type: "base64", media_type: part.mediaType, data: part.dataBase64 },
    });
  }
  return content;
}

function replayBlocks(value: JsonValue): JsonValue[] {
  const blocks = record(value)?.blocks;
  return Array.isArray(blocks) ? blocks.slice() : [];
}

function pushAnthropic(output: JsonValue[], role: string, content: JsonValue[]): void {
  const last = record(output.at(-1));
  if (last?.role === role && Array.isArray(last.content)) {
    last.content.push(...content);
    return;
  }
  output.push({ role, content });
}

function anthropicMessages(messages: LlmRequest["messages"]): JsonValue[] {
  const output: JsonValue[] = [];
  for (const message of messages) {
    if (message.role === "assistant") {
      const content: JsonValue[] = [];
      if (message.replayState?.providerKind === REPLAY_KIND) {
        content.push(...replayBlocks(message.replayState.value));
      }
      if (message.text) content.push({ type: "text", text: message.text });
      for (const toolCall of message.toolCalls) {
        content.push({
          type: "tool_use",
          id: toolCall.callId,
          name: toolCall.name,
          input: toolCall.arguments,
        });
      }
      if (content.length > 0) pushAnthropic(output, "assistant", content);
    } else if (message.role === "tool") {
      pushAnthropic(output, "user", [{
        type: "tool_result",
        tool_use_id: message.callId,
        content: message.parts.length === 0 ? message.content : anthropicParts(message.parts, true),
      }]);
    } else {
      const content = anthropicParts(message.content, message.role === "user");
      if (content.length > 0) pushAnthropic(output, "user", content);
    }
  }
  return output;
}

function markCacheBreakpoint(messages: JsonValue[]): void {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = record(messages[index]);
    if (message?.role !== "user" || !Array.isArray(message.content)) continue;
    for (let blockIndex = message.content.length - 1; blockIndex >= 0; blockIndex -= 1) {
      const block = record(message.content[blockIndex]);
      if (!block) continue;
      if (block.type !== "text" && block.type !== "image" && block.type !== "tool_result") continue;
      block.cache_control = CACHE_CONTROL;
      return;
    }
  }
}

export function buildMessagesBody(call: AnthropicCall): Record<string, JsonValue> {
  const messages = anthropicMessages(call.request.messages);
  markCacheBreakpoint(messages);
  const body: Record<string, JsonValue> = {
    model: call.model,
    system: call.request.instructions ? [{ type: "text", text: call.request.instructions, cache_control: CACHE_CONTROL }] : "",
    messages,
    max_tokens: call.request.maxOutputTokens ?? DEFAULT_MAX_TOKENS,
    stream: true,
  };
  if (call.request.tools.length > 0) {
    body.tools = call.request.tools.map((tool, index) => {
      const value: Record<string, JsonValue> = {
        name: tool.name,
        description: tool.description,
        input_schema: tool.parameters,
      };
      if (index + 1 === call.request.tools.length) value.cache_control = CACHE_CONTROL;
      return value;
    });
  }
  if (call.request.reasoning.enabled) {
    body.thinking = { type: "adaptive", display: "summarized" };
  }
  if (call.request.reasoning.effort !== null) {
    body.output_config = { effort: call.request.reasoning.effort };
  }
  return { ...body, ...call.extraBody };
}

function eventError(value: Record<string, unknown>): string | null {
  if (text(value.type) !== "error" && (value.error === undefined || value.error === null)) return null;
  const error = value.error;
  if (typeof error === "string") return error;
  const details = record(error);
  const kind = text(details?.type);
  const message = text(details?.message) ?? text(value.message);
  if (kind && message) return `${kind}: ${message}`;
  return message ?? kind ?? JSON.stringify(error ?? value);
}

function emptyUsage(): UsageState {
  return {
    inputTokens: null,
    outputTokens: null,
    totalTokens: null,
    cacheReadTokens: null,
    cacheWriteTokens: null,
    reasoningTokens: null,
  };
}

function anthropicUsage(value: unknown): UsageState {
  const usage = record(value) ?? {};
  return {
    inputTokens: count(usage.input_tokens),
    outputTokens: count(usage.output_tokens),
    totalTokens: count(usage.total_tokens),
    cacheReadTokens: count(usage.cache_read_input_tokens),
    cacheWriteTokens: count(usage.cache_creation_input_tokens),
    reasoningTokens: null,
  };
}

function mergeUsageField(total: number | null, update: number | null): number | null {
  if (update === null) return total;
  return total === null ? update : Math.max(total, update);
}

function mergeUsage(total: UsageState, update: UsageState): void {
  total.inputTokens = mergeUsageField(total.inputTokens, update.inputTokens);
  total.outputTokens = mergeUsageField(total.outputTokens, update.outputTokens);
  total.totalTokens = mergeUsageField(total.totalTokens, update.totalTokens);
  total.cacheReadTokens = mergeUsageField(total.cacheReadTokens, update.cacheReadTokens);
  total.cacheWriteTokens = mergeUsageField(total.cacheWriteTokens, update.cacheWriteTokens);
  const context = total.inputTokens === null ? null : total.inputTokens + (total.cacheReadTokens ?? 0) + (total.cacheWriteTokens ?? 0);
  if (context !== null && total.outputTokens !== null) total.totalTokens = context + total.outputTokens;
}

function mapStop(reason: string, sawTool: boolean): "stop" | "length" | "tool-use" {
  if (reason === "tool_use") return "tool-use";
  if (reason === "max_tokens" || reason === "model_context_window_exceeded") return "length";
  if (reason === "end_turn" || reason === "stop_sequence" || reason === "pause_turn" || reason === "refusal") {
    return "stop";
  }
  return sawTool ? "tool-use" : "stop";
}

function finalReason(finish: "stop" | "length" | "tool-use" | null, sawTool: boolean): "stop" | "length" | "tool-use" {
  if (finish === "length") return "length";
  if (sawTool) return "tool-use";
  return finish ?? "stop";
}

function closeBlock(
  index: number,
  kind: string,
  thinkingText: Map<number, string>,
  thinkingSignatures: Map<number, string>,
  thinkingBlocks: JsonValue[],
): ModelEvent[] {
  if (kind === "text") return [{ type: "text-end" }];
  if (kind === "thinking") {
    const thinking = thinkingText.get(index) ?? "";
    const signature = thinkingSignatures.get(index) ?? "";
    thinkingText.delete(index);
    thinkingSignatures.delete(index);
    // 空 thinking + 仅 signature 也要进回放;没有 signature 的块下一轮不能原样提交。
    if (signature) thinkingBlocks.push({ type: "thinking", thinking, signature });
    return [{ type: "thinking-end" }];
  }
  if (kind === "tool_use") return [{ type: "tool-call-end", index }];
  return [];
}

async function readBody(lines: AsyncIterable<string>): Promise<string> {
  const collected: string[] = [];
  for await (const line of lines) collected.push(line);
  return collected.join("\n");
}

/**
 * 执行一次 Messages 流式调用,发出与宿主统一事件集一致的标准化事件,
 * 包括文本/思考边界、工具参数增量与 thinking/redacted 回放状态。非 2xx 响应抛出
 * `HttpError`,流内失败抛出 `Error`,由调用方分类额度与授权问题。
 */
export async function streamAnthropic(
  call: AnthropicCall,
  output: ProviderOutput,
  context: PluginContext,
): Promise<void> {
  const response = await context.network.stream(call.url, {
    method: "POST",
    headers: {
      accept: "text/event-stream",
      "content-type": "application/json",
      ...call.headers,
      "anthropic-version": "2023-06-01",
    },
    body: JSON.stringify(buildMessagesBody(call)),
  });
  if (response.status < 200 || response.status >= 300) {
    throw new HttpError(response.status, await readBody(response.lines));
  }

  const blockTypes = new Map<number, string>();
  const thinkingText = new Map<number, string>();
  const thinkingSignatures = new Map<number, string>();
  const thinkingBlocks: JsonValue[] = [];
  const usage = emptyUsage();
  let sawUsage = false;
  let sawTool = false;
  let finish: "stop" | "length" | "tool-use" | null = null;
  let terminal = false;

  const emitAll = (events: ModelEvent[]) => {
    for (const event of events) output.emit(event);
  };
  const closeRemaining = () => {
    const remaining = [...blockTypes.entries()].sort((left, right) => left[0] - right[0]);
    blockTypes.clear();
    for (const [index, kind] of remaining) {
      emitAll(closeBlock(index, kind, thinkingText, thinkingSignatures, thinkingBlocks));
    }
  };
  const finishStream = () => {
    closeRemaining();
    if (thinkingBlocks.length > 0) {
      output.emit({ type: "replay-state", providerKind: REPLAY_KIND, value: { blocks: thinkingBlocks.slice() } });
      thinkingBlocks.length = 0;
    }
    if (sawUsage) output.emit({ type: "usage", usage: { ...usage } });
    output.emit({ type: "done", reason: finalReason(finish, sawTool) });
  };

  for await (const line of response.lines) {
    if (!line.startsWith("data:")) continue;
    const payload = line.slice(5).trim();
    if (!payload || payload === "[DONE]") continue;
    let value: Record<string, unknown>;
    try {
      value = record(JSON.parse(payload)) ?? {};
    } catch {
      throw new Error("Anthropic SSE returned invalid JSON");
    }
    const error = eventError(value);
    if (error !== null) throw new Error(`Anthropic error: ${error}`);
    switch (text(value.type)) {
      case "message_start": {
        const messageUsage = record(value.message)?.usage;
        if (messageUsage !== undefined) {
          sawUsage = true;
          mergeUsage(usage, anthropicUsage(messageUsage));
        }
        break;
      }
      case "content_block_start": {
        const index = requiredIndex(value);
        const block = record(value.content_block) ?? {};
        const kind = requiredType(block);
        blockTypes.set(index, kind);
        if (kind === "text") {
          output.emit({ type: "text-start" });
        } else if (kind === "thinking") {
          thinkingText.set(index, "");
          thinkingSignatures.set(index, "");
          output.emit({ type: "thinking-start" });
        } else if (kind === "redacted_thinking") {
          const data = text(block.data);
          if (data === null) throw new Error("Anthropic event is missing data");
          thinkingBlocks.push({ type: "redacted_thinking", data });
        } else if (kind === "tool_use") {
          const callId = text(block.id);
          const name = text(block.name);
          if (callId === null || name === null) throw new Error("Anthropic event is missing id or name");
          sawTool = true;
          output.emit({ type: "tool-call-start", index, callId, name });
        }
        break;
      }
      case "content_block_delta": {
        const index = requiredIndex(value);
        const delta = record(value.delta) ?? {};
        const deltaKind = requiredType(delta);
        if (!blockTypes.has(index)) {
          if (deltaKind === "text_delta") {
            blockTypes.set(index, "text");
            output.emit({ type: "text-start" });
          } else if (deltaKind === "thinking_delta" || deltaKind === "signature_delta") {
            blockTypes.set(index, "thinking");
            thinkingText.set(index, "");
            thinkingSignatures.set(index, "");
            output.emit({ type: "thinking-start" });
          }
        }
        if (deltaKind === "text_delta") {
          const deltaText = text(delta.text);
          if (deltaText !== null) output.emit({ type: "text-delta", text: deltaText });
        } else if (deltaKind === "thinking_delta") {
          const deltaText = text(delta.thinking);
          if (deltaText !== null) {
            thinkingText.set(index, (thinkingText.get(index) ?? "") + deltaText);
            output.emit({ type: "thinking-delta", text: deltaText });
          }
        } else if (deltaKind === "signature_delta") {
          const signature = text(delta.signature);
          if (signature !== null) {
            thinkingSignatures.set(index, (thinkingSignatures.get(index) ?? "") + signature);
          }
        } else if (deltaKind === "input_json_delta") {
          const partial = text(delta.partial_json);
          if (partial !== null) {
            sawTool = true;
            output.emit({ type: "tool-call-arguments-delta", index, delta: partial });
          }
        }
        break;
      }
      case "content_block_stop": {
        const index = requiredIndex(value);
        const kind = blockTypes.get(index);
        if (kind === undefined) break;
        blockTypes.delete(index);
        emitAll(closeBlock(index, kind, thinkingText, thinkingSignatures, thinkingBlocks));
        break;
      }
      case "message_delta": {
        if (value.usage !== undefined && value.usage !== null) {
          sawUsage = true;
          mergeUsage(usage, anthropicUsage(value.usage));
        }
        const reason = text(record(value.delta)?.stop_reason);
        if (reason !== null) finish = mapStop(reason, sawTool);
        break;
      }
      case "message_stop":
        terminal = true;
        finishStream();
        break;
    }
    if (terminal) break;
  }

  if (!terminal && finish !== null) {
    terminal = true;
    finishStream();
  }
  if (!terminal) throw new Error("Anthropic stream ended without message_stop");
}
