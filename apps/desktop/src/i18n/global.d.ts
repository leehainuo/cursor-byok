import type { TranslationParams } from "./runtime";

declare global {
  const t: (source: string, params?: TranslationParams) => string;
}

export {};
