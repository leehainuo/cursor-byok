import { useEffect } from "react";
import { CallTable } from "./CallTable";
import { PageContent } from "../../shell/layout/PageContent";
import { appStore, useAppStore } from "../../shared/store/appStore";
import styles from "./CallsPage.module.scss";

const CALL_REFRESH_INTERVAL_MS = 2_000;

export function CallsPage() {
  const { calls } = useAppStore();

  useEffect(() => {
    let disposed = false;
    const refreshCalls = () => {
      if (!disposed && document.visibilityState === "visible") {
        void appStore.refreshCalls();
      }
    };

    refreshCalls();
    const interval = window.setInterval(refreshCalls, CALL_REFRESH_INTERVAL_MS);
    window.addEventListener("focus", refreshCalls);
    document.addEventListener("visibilitychange", refreshCalls);
    return () => {
      disposed = true;
      window.clearInterval(interval);
      window.removeEventListener("focus", refreshCalls);
      document.removeEventListener("visibilitychange", refreshCalls);
    };
  }, []);

  const content = <div className={styles.page}><CallTable calls={calls} onDetails={(call) => void appStore.openCallDetails(call.call_id)} /></div>;
  return <PageContent fixed title={t("调用")} contentClassName={styles.pageContent} sections={[{ key: "calls", estimatedHeight: 720, content }]} />;
}
