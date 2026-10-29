import type { Model } from "../api";

export function modelProviderName(model: Pick<Model, "group_name" | "base_url">): string {
  const configuredName = model.group_name?.trim();
  if (configuredName) return configuredName;

  const value = model.base_url.trim();
  try {
    return new URL(value).hostname.toLowerCase() || value;
  } catch {
    try {
      return new URL(`https://${value}`).hostname.toLowerCase() || value;
    } catch {
      return value;
    }
  }
}
