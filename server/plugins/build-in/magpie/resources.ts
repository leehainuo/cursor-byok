import type { JsonValue } from "cursor-byok:plugin";
import type {
  FormAddMethod,
  ResourceDraft,
  ResourceSnapshot,
  ResourceView,
} from "cursor-byok:resource";

export const RESOURCE_TYPE = "gateway";
export const CONNECT_METHOD_ID = "connect";
export const DEFAULT_ORIGIN = "http://127.0.0.1:3425";
export const DEFAULT_API_KEY = "magpie-cursor-byok";

export type GatewayData = {
  origin: string;
  apiKey: string;
};

function object(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function text(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

/** 从用户填写的网关 URL 取出 origin;路径忽略,凭证与非 HTTP 拒绝。 */
export function gatewayOrigin(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) throw new Error("Magpie gateway URL is required");
  let url: URL;
  try {
    url = new URL(trimmed);
  } catch {
    throw new Error("invalid Magpie gateway URL");
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("Magpie gateway URL must be http or https");
  }
  if (url.username || url.password) {
    throw new Error("Magpie gateway URL must not contain credentials");
  }
  if (!url.hostname) throw new Error("Magpie gateway URL has no host");
  return url.origin;
}

export function gatewayData(resource: ResourceSnapshot): GatewayData {
  const data = object(resource.privateData);
  const origin = data ? gatewayOrigin(text(data.origin) ?? "") : "";
  const apiKey = text(data?.apiKey);
  if (!apiKey) throw new Error("Magpie gateway resource is missing its API key");
  return { origin, apiKey };
}

export function gatewayHeaders(apiKey: string): Record<string, string> {
  return {
    accept: "application/json",
    authorization: `Bearer ${apiKey}`,
    "x-api-key": apiKey,
  };
}

export function presentGateway(resource: ResourceSnapshot): ResourceView {
  return { displayName: gatewayData(resource).origin };
}

export const magpieConnect: FormAddMethod = {
  type: "form",
  id: CONNECT_METHOD_ID,
  displayName: { "en-US": "Connect Magpie", "zh-CN": "连接 Magpie" },
  fields: [
    {
      id: "base-url",
      label: { "en-US": "Gateway URL", "zh-CN": "网关地址" },
      type: "url",
      required: true,
      placeholder: DEFAULT_ORIGIN,
    },
    {
      id: "api-key",
      label: { "en-US": "API key", "zh-CN": "API 密钥" },
      type: "secret",
      required: true,
      placeholder: DEFAULT_API_KEY,
    },
  ],
  submit: (values): Promise<ResourceDraft[]> => {
    const origin = gatewayOrigin(values["base-url"] ?? "");
    const apiKey = text(values["api-key"]);
    if (!apiKey) throw new Error("Magpie API key is required");
    const data: GatewayData = { origin, apiKey };
    return Promise.resolve([{ key: origin, privateData: data as unknown as JsonValue }]);
  },
};
