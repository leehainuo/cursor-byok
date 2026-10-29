import type { AdSlot } from "./types";

const adCacheStorageKey = "cursor-byok:ad-runtime";

export function loadCachedAds(): AdSlot[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(adCacheStorageKey) ?? "null");
    if (!value || typeof value !== "object" || !Array.isArray((value as { slots?: unknown }).slots)) return [];
    return (value as { slots: AdSlot[] }).slots;
  } catch {
    return [];
  }
}

export function saveCachedAds(slots: AdSlot[]): void {
  try {
    localStorage.setItem(adCacheStorageKey, JSON.stringify({ slots }));
  } catch {
    // The current session still uses the fetched ads when storage is unavailable.
  }
}
