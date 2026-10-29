export type DesktopPlatform = "macos" | "windows" | "linux";

export function desktopPlatform(): DesktopPlatform {
  const configured = document.documentElement.dataset.platform;
  if (configured === "macos" || configured === "windows" || configured === "linux") {
    return configured;
  }

  const agent = navigator.userAgent;
  if (/Macintosh|Mac OS X/.test(agent)) return "macos";
  if (/Windows/.test(agent)) return "windows";
  return "linux";
}
