import { Outlet } from "react-router-dom";
import { desktopPlatform } from "../shared/native/platform";
import styles from "./AppFrame.module.scss";
import { AppHeader } from "./AppHeader";

const currentPlatform = desktopPlatform();
document.documentElement.dataset.platform = currentPlatform;

export function AppFrame() {
  const platform = currentPlatform;
  const platformClasses = platform === "windows" ? [styles.windows] : [];
  const nativeDesktop = "__TAURI_INTERNALS__" in window;

  return (
    <div className={[styles.shell, ...platformClasses].filter(Boolean).join(" ")}>
      <AppHeader platform={platform} nativeDesktop={nativeDesktop} />
      <Outlet />
    </div>
  );
}
