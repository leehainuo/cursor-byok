import type { NetworkEventStream, NetworkResponse, PluginContext } from "cursor-byok:plugin";
import type { LlmRequest, ModelEvent } from "cursor-byok:provider";
import type { ResourceSnapshot } from "cursor-byok:resource";
import { parseMagpieModels } from "./catalog.ts";
import { magpieProvider, selectProtocol } from "./provider.ts";
import {
  CONNECT_METHOD_ID,
  DEFAULT_API_KEY,
  DEFAULT_ORIGIN,
  magpieConnect,
  presentGateway,
  RESOURCE_TYPE,
} from "./resources.ts";

function assert(condition: unknown, message = "assertion failed"): asserts condition {
  if (!condition) throw new Error(message);
}

function assertEquals(actual: unknown, expected: unknown): void {
  const left = JSON.stringify(actual);
  const right = JSON.stringify(expected);
  if (left !== right) throw new Error(`expected ${right}, received ${left}`);
}

type RequestInit = { body?: string; headers?: Record<string, string> };
type FetchHandler = (url: string, init?: RequestInit) => NetworkResponse;
type StreamHandler = (url: string, init?: RequestInit) => NetworkEventStream;

function context(handlers: { fetch?: FetchHandler; stream?: StreamHandler }): PluginContext {
  return {
    network: {
      fetch: (url, init) => {
        if (!handlers.fetch) throw new Error("fetch was not expected");
        return Promise.resolve(handlers.fetch(url, init));
      },
      stream: (url, init) => {
        if (!handlers.stream) throw new Error("stream was not expected");
        return Promise.resolve(handlers.stream(url, init));
      },
    },
    signal: new AbortController().signal,
  };
}

function snapshot(origin: string, apiKey = "magpie-cursor-byok"): ResourceSnapshot {
  return {
    id: "resource-1",
    type: RESOURCE_TYPE,
    key: origin,
    privateData: { origin, apiKey },
    state: { status: "ready" },
  };
}

async function* sse(lines: string[]): AsyncGenerator<string> {
  for (const line of lines) yield line;
}

function request(): LlmRequest {
  return {
    instructions: "You are a coding assistant.",
    messages: [{ role: "user", content: [{ type: "text", text: "hi" }] }],
    tools: [],
    reasoning: { enabled: false, effort: null },
    latency: "standard",
    maxOutputTokens: 32_000,
    cacheKey: "conversation-1",
  };
}

Deno.test("form submit normalizes origin as the resource key and does not fetch", async () => {
  const drafts = await magpieConnect.submit({
    "base-url": "http://127.0.0.1:3425/",
    "api-key": DEFAULT_API_KEY,
  }, context({}));
  assertEquals(drafts, [{
    key: DEFAULT_ORIGIN,
    privateData: { origin: DEFAULT_ORIGIN, apiKey: DEFAULT_API_KEY },
  }]);
  const view = presentGateway(snapshot(DEFAULT_ORIGIN, DEFAULT_API_KEY));
  assertEquals(view.displayName, DEFAULT_ORIGIN);
  assert(!JSON.stringify(view).includes(DEFAULT_API_KEY), "resource view exposed the API key");
});

Deno.test("form submit keeps distinct LAN origins and rejects credentials", async () => {
  const drafts = await magpieConnect.submit({
    "base-url": "http://192.168.1.2:3425/v1",
    "api-key": "magpie-lan",
  }, context({}));
  assertEquals(drafts[0].key, "http://192.168.1.2:3425");
  let failed = false;
  try {
    await magpieConnect.submit({
      "base-url": "http://user:pass@127.0.0.1:3425",
      "api-key": "k",
    }, context({}));
  } catch (error) {
    failed = error instanceof Error && error.message.includes("credentials");
  }
  assert(failed, "credentialed URLs must be rejected");
  assertEquals(magpieConnect.id, CONNECT_METHOD_ID);
});

Deno.test("catalog keeps chat models, drops image/video, and preserves native endpoints", () => {
  const models = parseMagpieModels({
    data: [
      {
        id: "deepseek/deepseek-v4-pro",
        display_name: "DeepSeek V4 Pro",
        magpie_label: "DeepSeek V4 Pro · local",
        native_endpoints: ["/v1/chat/completions"],
        max_output_tokens: 8192,
      },
      {
        id: "openai/gpt-5",
        display_name: "GPT-5",
        native_endpoints: ["/v1/responses", "/v1/chat/completions"],
      },
      {
        id: "anthropic/claude-sonnet-4-5",
        display_name: "Claude Sonnet",
        native_endpoints: ["/v1/messages"],
      },
      { id: "black-forest/flux", kind: "image" },
      { id: "openai/sora", kind: "video" },
      { id: "deepseek/deepseek-v4-pro", display_name: "duplicate" },
    ],
  });
  assertEquals(models, [
    {
      id: "deepseek/deepseek-v4-pro",
      displayName: "DeepSeek V4 Pro · local",
      maxOutputTokens: 8192,
      privateData: { nativeEndpoints: ["/v1/chat/completions"] },
    },
    {
      id: "openai/gpt-5",
      displayName: "GPT-5",
      privateData: { nativeEndpoints: ["/v1/responses", "/v1/chat/completions"] },
    },
    {
      id: "anthropic/claude-sonnet-4-5",
      displayName: "Claude Sonnet",
      privateData: { nativeEndpoints: ["/v1/messages"] },
    },
  ]);
  assertEquals(parseMagpieModels({ data: [] }), []);
});

Deno.test("catalog list calls GET /v1/models on the bound origin", async () => {
  let url = "";
  let headers: Record<string, string> = {};
  const models = await magpieProvider.models?.list(
    { resource: snapshot("http://192.168.1.2:3425", "magpie-lan") },
    context({
      fetch: (requested, init) => {
        url = requested;
        headers = init?.headers ?? {};
        return { status: 200, headers: {}, body: JSON.stringify({ data: [] }) };
      },
    }),
  );
  assertEquals(url, "http://192.168.1.2:3425/v1/models");
  assertEquals(headers.authorization, "Bearer magpie-lan");
  assertEquals(models, []);
});

Deno.test("native_endpoints select Responses, Messages, or Chat fallback", () => {
  assertEquals(selectProtocol(["/v1/responses", "/v1/chat/completions"]), "responses");
  assertEquals(selectProtocol(["/v1/messages"]), "messages");
  assertEquals(selectProtocol(["/v1/chat/completions"]), "chat");
  assertEquals(selectProtocol([]), "chat");
  assertEquals(selectProtocol(["/v1beta/models/gemini-pro:generateContent"]), "chat");
});

Deno.test("invoke uses the bound origin, bare Magpie id, and selected protocol", async () => {
  const cases: Array<{
    endpoints: string[];
    url: string;
    model: string;
    lines: string[];
    bodyKey: string;
  }> = [
    {
      endpoints: ["/v1/responses"],
      url: "http://127.0.0.1:3425/v1/responses",
      model: "openai/gpt-5",
      lines: ['data: {"type":"response.completed","response":{}}'],
      bodyKey: "input",
    },
    {
      endpoints: ["/v1/messages"],
      url: "http://127.0.0.1:3425/v1/messages",
      model: "anthropic/claude-sonnet-4-5",
      lines: [
        'data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}',
        'data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ok"}}',
        'data: {"type":"content_block_stop","index":0}',
        'data: {"type":"message_delta","delta":{"stop_reason":"end_turn"}}',
        'data: {"type":"message_stop"}',
      ],
      bodyKey: "messages",
    },
    {
      endpoints: [],
      url: "http://127.0.0.1:3425/v1/chat/completions",
      model: "google/gemini-2.5-pro",
      lines: [
        'data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}]}',
        "data: [DONE]",
      ],
      bodyKey: "messages",
    },
  ];
  for (const item of cases) {
    let url = "";
    let body: Record<string, unknown> = {};
    const events: ModelEvent[] = [];
    const result = await magpieProvider.invoke(
      {
        model: {
          id: item.model,
          displayName: item.model,
          privateData: { nativeEndpoints: item.endpoints },
        },
        resource: snapshot(DEFAULT_ORIGIN),
        request: request(),
      },
      { emit: (event) => events.push(event) },
      context({
        stream: (requested, init) => {
          url = requested;
          body = JSON.parse(init?.body ?? "{}") as Record<string, unknown>;
          return { status: 200, headers: {}, lines: sse(item.lines) };
        },
      }),
    );
    assertEquals(result, { status: "completed" });
    assertEquals(url, item.url);
    assertEquals(body.model, item.model);
    assert(item.bodyKey in body, `${item.url} missing ${item.bodyKey}`);
    assert(events.some((event) => event.type === "done"), `${item.url} did not complete`);
  }
});

Deno.test("invoke maps 401 to an invalid resource", async () => {
  const result = await magpieProvider.invoke(
    {
      model: { id: "openai/gpt-5", displayName: "GPT-5", privateData: { nativeEndpoints: [] } },
      resource: snapshot(DEFAULT_ORIGIN),
      request: request(),
    },
    { emit: () => {} },
    context({
      stream: () => ({
        status: 401,
        headers: {},
        lines: sse(["unauthorized"]),
      }),
    }),
  );
  assert(result.status === "resource-error", `expected resource-error, received ${result.status}`);
  assertEquals(result.patch.state?.status, "invalid");
});
