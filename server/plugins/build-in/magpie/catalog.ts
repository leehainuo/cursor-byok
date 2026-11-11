import type { JsonValue, PluginContext } from "cursor-byok:plugin";
import type { ModelDefinition, ModelSupport } from "cursor-byok:model";
import { gatewayData, gatewayHeaders } from "./resources.ts";

function object(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function text(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function positiveInteger(value: unknown): number | null {
  const parsed = typeof value === "number"
    ? value
    : typeof value === "string"
    ? Number(value)
    : NaN;
  return Number.isFinite(parsed) && parsed > 0 ? Math.floor(parsed) : null;
}

function nativeEndpoints(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return [
    ...new Set(
      value.filter((item): item is string => typeof item === "string" && item.trim() !== ""),
    ),
  ];
}

function isMediaKind(value: unknown): boolean {
  const kind = text(value)?.toLowerCase();
  return kind === "image" || kind === "video";
}

export function parseMagpieModels(body: unknown): ModelDefinition[] {
  const root = object(body);
  const source = root?.data ?? root?.models ?? body;
  if (!Array.isArray(source)) {
    throw new Error("Magpie model discovery response does not contain a model list");
  }
  const seen = new Set<string>();
  const models: ModelDefinition[] = [];
  for (const raw of source) {
    const model = object(raw);
    if (!model || isMediaKind(model.kind)) continue;
    const id = text(model.id);
    if (!id || seen.has(id)) continue;
    seen.add(id);
    const displayName = text(model.magpie_label) ??
      text(model.display_name) ??
      text(model.displayName) ??
      id;
    const maxOutputTokens = positiveInteger(model.max_output_tokens ?? model.maxOutputTokens);
    models.push({
      id,
      displayName,
      ...(maxOutputTokens !== null ? { maxOutputTokens } : {}),
      privateData: {
        nativeEndpoints: nativeEndpoints(model.native_endpoints ?? model.nativeEndpoints),
      },
    });
  }
  return models;
}

export const magpieModels: ModelSupport = {
  catalog: "live",
  list: async ({ resource }, context: PluginContext): Promise<ModelDefinition[]> => {
    if (!resource) throw new Error("add a Magpie gateway before listing models");
    const data = gatewayData(resource);
    const response = await context.network.fetch(`${data.origin}/v1/models`, {
      method: "GET",
      headers: gatewayHeaders(data.apiKey),
    });
    if (response.status < 200 || response.status >= 300) {
      throw new Error(`Magpie model discovery failed (HTTP ${response.status}): ${response.body}`);
    }
    let body: unknown;
    try {
      body = JSON.parse(response.body) as JsonValue;
    } catch {
      throw new Error("Magpie model discovery returned invalid JSON");
    }
    return parseMagpieModels(body);
  },
};
