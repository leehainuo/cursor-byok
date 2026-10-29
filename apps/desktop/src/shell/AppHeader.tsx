import { useEffect, useState } from "react";
import appIcon from "../../src-tauri/icons/32x32.png";
import { currentAppVersion } from "../shared/native/appLifecycle";
import type { DesktopPlatform } from "../shared/native/platform";
import { WindowControls } from "./WindowControls";
import { MacTrafficLights } from "./MacTrafficLights";
import styles from "./AppHeader.module.scss";

type AppHeaderProps = {
  platform: DesktopPlatform;
  nativeDesktop: boolean;
};

export function AppHeader({ platform, nativeDesktop }: AppHeaderProps) {
  const showMacTrafficLights = !nativeDesktop && platform === "macos";
  const showNativeUi = nativeDesktop && platform !== "macos";
  const [version, setVersion] = useState("…");

  useEffect(() => {
    let disposed = false;
    void currentAppVersion().then((next) => {
      if (!disposed) setVersion(next);
    });
    return () => { disposed = true; };
  }, []);

  return <header className={styles.root}>
    <div className={styles.dragLayer} data-tauri-drag-region aria-hidden="true" />
    <div className={styles.uiLayer}>
      {showMacTrafficLights && <MacTrafficLights />}
      {showNativeUi && <>
        <div className={styles.identity} aria-label="Cursor BYOK">
          <img src={appIcon} alt="" />
          <span>{t("Cursor 助手 v{version}", { version })}</span>
        </div>
        <WindowControls />
      </>}
    </div>
  </header>;
}
