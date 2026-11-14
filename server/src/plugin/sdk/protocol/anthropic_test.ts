import type { NetworkEventStream, PluginContext } from "../plugin.ts";
import type { LlmRequest, ModelEvent } from "../provider.ts";
import { buildMessagesBody, HttpError, REPLAY_KIND, streamAnthropic } from "./anthropic.ts";

function assert(condition: unknown, message = "assertion failed"): asserts condition {
  if (!condition) throw new Error(message);
}

function assertEquals(actual: unknown, expected: unknown): void {
  const left = JSON.stringify(actual);
  const right = JSON.stringify(expected);
  if (left !== right) throw new Error(`expected ${right}, received ${left}`);
}

function request(overrides: Partial<LlmRequest> = {}): LlmRequest {
  return {
    instructions: "You are concise.",
    messages: [{ role: "user", content: [{ type: "text", text: "hello" }] }],
    tools: [],
    reasoning: { enabled: false, effort: null },
    latency: "standard",
    maxOutputTokens: null,
    cacheKey: "conversation-1",
    ...overrides,
  };
}

function call(overrides: Partial<LlmRequest> = {}) {
  return {
    url: "https://api.anthropic.com/v1/messages",
    model: "claude-sonnet-4-5",
    request: request(overrides),
  };
}

async function* sse(lines: string[]): AsyncGenerator<string> {
  for (const line of lines) yield line;
}

function context(handler: (url: string, init?: { body?: string; headers?: Record<string, string> }) => NetworkEventStream): PluginContext {
  return {
    network: {
      fetch: () => {
        throw new Error("fetch was not expected");
      },
      stream: (url, init) => Promise.resolve(handler(url, init)),
    },
    signal: new AbortController().signal,
  };
}

async function collect(
  lines: string[],
  overrides: Partial<LlmRequest> = {},
  inspect?: (url: string, init?: { body?: string; headers?: Record<string, string> }) => void,
): Promise<ModelEvent[]> {
  const events: ModelEvent[] = [];
  await streamAnthropic(
    call(overrides),
    { emit: (event) => events.push(event) },
    context((url, init) => {
      inspect?.(url, init);
      return { status: 200, headers: {}, lines: sse(lines) };
    }),
  );
  return events;
}

Deno.test("buildMessagesBody marks cache breakpoints and defaults max_tokens", () => {
  const body = buildMessagesBody(call({
    tools: [{ name: "lookup", description: "Look up", parameters: { type: "object" } }],
    reasoning: { enabled: true, effort: "high" },
    maxOutputTokens: null,
  }));
  assertEquals(body.model, "claude-sonnet-4-5");
  assertEquals(body.stream, true);
  assertEquals(body.max_tokens, 65_000);
  assertEquals(body.system, [{
    type: "text",
    text: "You are concise.",
    cache_control: { type: "ephemeral" },
  }]);
  assertEquals(body.tools, [{
    name: "lookup",
    description: "Look up",
    input_schema: { type: "object" },
    cache_control: { type: "ephemeral" },
  }]);
  assertEquals(body.messages, [{
    role: "user",
    content: [{ type: "text", text: "hello", cache_control: { type: "ephemeral" } }],
  }]);
  assertEquals(body.thinking, { type: "adaptive", display: "summarized" });
  assertEquals(body.output_config, { effort: "high" });
  assert(!("prompt_cache_key" in body));
  assert(!("stream_options" in body));
});

Deno.test("buildMessagesBody replays thinking blocks and merges consecutive roles", () => {
  const body = buildMessagesBody(call({
    instructions: "",
    messages: [
      { role: "system", content: [{ type: "text", text: "sys" }] },
      { role: "user", content: [{ type: "text", text: "ask" }] },
      {
        role: "assistant",
        text: "ok",
        thinking: "",
        replayState: {
          providerKind: REPLAY_KIND,
          value: {
            blocks: [
              { type: "thinking", thinking: "", signature: "sig" },
              { type: "redacted_thinking", data: "opaque" },
            ],
          },
        },
        toolCalls: [{ index: 0, callId: "call-1", name: "lookup", arguments: { q: "weather" } }],
      },
      {
        role: "tool",
        callId: "call-1",
        name: "lookup",
        content: "sunny",
        isError: false,
        parts: [],
      },
      { role: "user", content: [{ type: "text", text: "thanks" }] },
    ],
  }));
  assertEquals(body.system, "");
  assertEquals(body.messages, [
    {
      role: "user",
      content: [
        { type: "text", text: "sys" },
        { type: "text", text: "ask" },
      ],
    },
    {
      role: "assistant",
      content: [
        { type: "thinking", thinking: "", signature: "sig" },
        { type: "redacted_thinking", data: "opaque" },
        { type: "text", text: "ok" },
        { type: "tool_use", id: "call-1", name: "lookup", input: { q: "weather" } },
      ],
    },
    {
      role: "user",
      content: [
        { type: "tool_result", tool_use_id: "call-1", content: "sunny" },
        { type: "text", text: "thanks", cache_control: { type: "ephemeral" } },
      ],
    },
  ]);
});

Deno.test("buildMessagesBody accepts user images and rejects system images", () => {
  const withImage = buildMessagesBody(call({
    messages: [{
      role: "user",
      content: [
        { type: "text", text: "see" },
        { type: "image", mediaType: "image/png", dataBase64: "AAECAw==" },
      ],
    }],
  }));
  assertEquals(withImage.messages, [{
    role: "user",
    content: [
      { type: "text", text: "see" },
      {
        type: "image",
        source: { type: "base64", media_type: "image/png", data: "AAECAw==" },
        cache_control: { type: "ephemeral" },
      },
    ],
  }]);

  let threw = false;
  try {
    buildMessagesBody(call({
      messages: [{
        role: "system",
        content: [{ type: "image", mediaType: "image/png", dataBase64: "AAECAw==" }],
      }],
    }));
  } catch (error) {
    threw = true;
    assertEquals((error as Error).message, "Anthropic only accepts images in user messages");
  }
  assert(threw);
});

Deno.test("streamAnthropic maps text, thinking, usage, and stop", async () => {
  let headers: Record<string, string> = {};
  const events = await collect(
    [
      'data: {"type":"message_start","message":{"usage":{"input_tokens":10,"cache_read_input_tokens":20,"cache_creation_input_tokens":30}}}',
      'data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}',
      'data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"why"}}',
      'data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}}',
      'data: {"type":"content_block_stop","index":0}',
      'data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}',
      'data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hi"}}',
      'data: {"type":"content_block_stop","index":1}',
      'data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}',
      'data: {"type":"message_stop"}',
    ],
    {},
    (_url, init) => {
      headers = init?.headers ?? {};
    },
  );
  assertEquals(headers["anthropic-version"], "2023-06-01");
  assertEquals(events, [
    { type: "thinking-start" },
    { type: "thinking-delta", text: "why" },
    { type: "thinking-end" },
    { type: "text-start" },
    { type: "text-delta", text: "Hi" },
    { type: "text-end" },
    {
      type: "replay-state",
      providerKind: REPLAY_KIND,
      value: { blocks: [{ type: "thinking", thinking: "why", signature: "sig" }] },
    },
    {
      type: "usage",
      usage: {
        inputTokens: 10,
        outputTokens: 2,
        totalTokens: 62,
        cacheReadTokens: 20,
        cacheWriteTokens: 30,
        reasoningTokens: null,
      },
    },
    { type: "done", reason: "stop" },
  ]);
});

Deno.test("streamAnthropic replays empty thinking with signature and redacted blocks", async () => {
  const events = await collect([
    'data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}',
    'data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig-only"}}',
    'data: {"type":"content_block_stop","index":0}',
    'data: {"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":"opaque"}}',
    'data: {"type":"content_block_stop","index":1}',
    'data: {"type":"message_delta","delta":{"stop_reason":"end_turn"}}',
    'data: {"type":"message_stop"}',
  ]);
  assertEquals(events, [
    { type: "thinking-start" },
    { type: "thinking-end" },
    {
      type: "replay-state",
      providerKind: REPLAY_KIND,
      value: {
        blocks: [
          { type: "thinking", thinking: "", signature: "sig-only" },
          { type: "redacted_thinking", data: "opaque" },
        ],
      },
    },
    { type: "done", reason: "stop" },
  ]);
});

Deno.test("streamAnthropic maps tool_use and length stop reasons", async () => {
  const events = await collect([
    'data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call-1","name":"lookup"}}',
    'data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\\"q\\":"}}',
    'data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"\\"a\\"}"}}',
    'data: {"type":"content_block_stop","index":0}',
    'data: {"type":"message_delta","delta":{"stop_reason":"max_tokens"}}',
    'data: {"type":"message_stop"}',
  ]);
  assertEquals(events, [
    { type: "tool-call-start", index: 0, callId: "call-1", name: "lookup" },
    { type: "tool-call-arguments-delta", index: 0, delta: '{"q":' },
    { type: "tool-call-arguments-delta", index: 0, delta: '"a"}' },
    { type: "tool-call-end", index: 0 },
    { type: "done", reason: "length" },
  ]);
});

Deno.test("streamAnthropic finishes after message_delta when message_stop is omitted", async () => {
  const events = await collect([
    'data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}',
    'data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}',
    'data: {"type":"message_delta","delta":{"stop_reason":"tool_use"}}',
  ]);
  assertEquals(events, [
    { type: "text-start" },
    { type: "text-delta", text: "Hi" },
    { type: "text-end" },
    { type: "done", reason: "tool-use" },
  ]);
});

Deno.test("streamAnthropic throws HttpError and missing message_stop", async () => {
  try {
    await streamAnthropic(
      call(),
      { emit: () => {} },
      context(() => ({ status: 401, headers: {}, lines: sse(["unauthorized"]) })),
    );
    throw new Error("expected HttpError");
  } catch (error) {
    assert(error instanceof HttpError);
    assertEquals(error.status, 401);
    assertEquals(error.body, "unauthorized");
  }

  try {
    await collect(['data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}']);
    throw new Error("expected missing message_stop");
  } catch (error) {
    assertEquals((error as Error).message, "Anthropic stream ended without message_stop");
  }
});
