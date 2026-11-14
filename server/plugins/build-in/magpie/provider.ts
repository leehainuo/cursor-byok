import type {
  ProviderInvokeInput,
  ProviderOutput,
  ProviderResult,
  ProviderSupport,
} from "cursor-byok:provider";
import type { PluginContext } from "cursor-byok:plugin";
import type { ModelSnapshot } from "cursor-byok:model";
import { HttpError as ChatHttpError, streamOpenAiChat } from "cursor-byok:protocol/openai-chat";
import {
  HttpError as ResponsesHttpError,
  streamOpenAiResponses,
} from "cursor-byok:protocol/openai-responses";
import { HttpError as AnthropicHttpError, streamAnthropic } from "cursor-byok:protocol/anthropic";
import { magpieModels } from "./catalog.ts";
import { gatewayData, gatewayHeaders, RESOURCE_TYPE } from "./resources.ts";

export type MagpieProtocol = "responses" | "messages" | "chat";

function object(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function nativeEndpoints(model: ModelSnapshot): string[] {
  const data = object(model.privateData);
  const value = data?.nativeEndpoints;
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];
}

/** 缓存没有 native_endpoints 时回退 Chat,让 Magpie 翻译未知模型。 */
export function selectProtocol(endpoints: string[]): MagpieProtocol {
  if (endpoints.some((path) => path.includes("/v1/responses"))) return "responses";
  if (endpoints.some((path) => path.includes("/v1/messages"))) return "messages";
  return "chat";
}

export function endpointFor(
  origin: string,
  endpoints: string[],
): { protocol: MagpieProtocol; url: string } {
  const protocol = selectProtocol(endpoints);
  if (protocol === "responses") return { protocol, url: `${origin}/v1/responses` };
  if (protocol === "messages") return { protocol, url: `${origin}/v1/messages` };
  return { protocol, url: `${origin}/v1/chat/completions` };
}

function invalidResult(message: string, stateMessage: string): ProviderResult {
  return {
    status: "resource-error",
    message,
    patch: { state: { status: "invalid", message: stateMessage } },
  };
}

function requestError(error: unknown): ProviderResult {
  if (
    error instanceof ChatHttpError || error instanceof ResponsesHttpError ||
    error instanceof AnthropicHttpError
  ) {
    if (error.status === 401) {
      return invalidResult(error.message, "Magpie authorization failed; check the API key");
    }
    return { status: "request-error", message: error.message };
  }
  const message = error instanceof Error ? error.message : String(error);
  return { status: "request-error", message };
}

async function invoke(
  input: ProviderInvokeInput,
  output: ProviderOutput,
  context: PluginContext,
): Promise<ProviderResult> {
  if (!input.resource) {
    return { status: "request-error", message: "add a Magpie gateway before calling Magpie" };
  }
  let origin: string;
  let headers: Record<string, string>;
  try {
    const data = gatewayData(input.resource);
    origin = data.origin;
    headers = gatewayHeaders(data.apiKey);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return invalidResult(message, message);
  }
  const { protocol, url } = endpointFor(origin, nativeEndpoints(input.model));
  const call = { url, model: input.model.id, request: input.request, headers };
  try {
    if (protocol === "responses") await streamOpenAiResponses(call, output, context);
    else if (protocol === "messages") await streamAnthropic(call, output, context);
    else await streamOpenAiChat(call, output, context);
    return { status: "completed" };
  } catch (error) {
    return requestError(error);
  }
}

export const magpieProvider: ProviderSupport = {
  id: "magpie",
  displayName: "Magpie",
  description: {
    "en-US": "Live models from a Magpie gateway, forwarded on native Responses, Messages, or Chat.",
    "zh-CN":
      "从 Magpie 网关实时读取目录,并按 native_endpoints 原样转发 Responses、Messages 或 Chat。",
  },
  providerType: "magpie",
  resourceType: RESOURCE_TYPE,
  models: magpieModels,
  invoke,
};
