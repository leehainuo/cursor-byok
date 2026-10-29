export const themeIds = ["default-dark", "default-light"] as const;
export type ThemeId = (typeof themeIds)[number];

export const themeOptions = [
  { id: "default-dark" },
  { id: "default-light" },
] satisfies { id: ThemeId }[];

export function isThemeId(value: string | null): value is ThemeId {
  return value !== null && themeIds.some((id) => id === value);
}

export function applyTheme(themeId: ThemeId) {
  document.documentElement.dataset.theme = themeId;
}
