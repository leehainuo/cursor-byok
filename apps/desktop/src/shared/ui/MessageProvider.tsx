import { useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { getMessageSnapshot, subscribeToMessages } from "./message";
import styles from "./MessageProvider.module.scss";

export function MessageProvider() {
  const { current } = useSyncExternalStore(subscribeToMessages, getMessageSnapshot);

  return createPortal(
    <div className={styles.region} aria-live="polite" aria-atomic="true">
      {current && (
        <div key={current.id} className={`${styles.message} ${current.leaving ? styles.leaving : ""}`} role="status">
          {current.content}
        </div>
      )}
    </div>,
    document.body,
  );
}
