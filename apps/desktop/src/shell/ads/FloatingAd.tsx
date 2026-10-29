import { useEffect, useMemo, useRef } from "react";
import { createPortal } from "react-dom";
import { Icon } from "../../shared/ui/Icon";
import { windowCloseIcon } from "../../shared/ui/icons";
import { VirtualList } from "../../shared/virtual/VirtualList";
import type { AdAction, AdSlot } from "./types";
import styles from "./Ads.module.scss";

type AdTextSection =
  | { key: "intro"; kind: "intro" }
  | { key: string; kind: "detail"; label: string; value: string };

type FloatingAdProps = {
  ad: AdSlot;
  trigger: HTMLButtonElement | null;
  onClose: () => void;
  onAction: (action: AdAction) => void;
};

export function FloatingAd({ ad, trigger, onClose, onAction }: FloatingAdProps) {
  const dialog = useRef<HTMLDivElement>(null);
  const textSections = useMemo<AdTextSection[]>(() => [
    { key: "intro", kind: "intro" },
    ...ad.content.details.map((detail, index) => ({
      key: `detail-${index}-${detail.label}`,
      kind: "detail" as const,
      label: detail.label,
      value: detail.value,
    })),
  ], [ad.content.details]);

  useEffect(() => {
    dialog.current?.focus();
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!dialog.current?.contains(target) && !trigger?.contains(target)) onClose();
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      onClose();
      trigger?.focus();
    };
    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [onClose, trigger]);

  return createPortal(<div
    id="menu-ad-dialog"
    ref={dialog}
    className={styles.floatingAd}
    role="dialog"
    aria-modal="false"
    aria-label={ad.content.title}
    tabIndex={-1}
  >
    <div className={styles.hero}>
      <img src={ad.content.imageUrl} alt="" />
      <span className={styles.promotionLabel}>{t("推广")}</span>
      <button type="button" className={styles.close} aria-label={t("关闭广告")} onClick={() => { onClose(); trigger?.focus(); }}>
        <Icon icon={windowCloseIcon} size="1.1em" />
      </button>
    </div>
    <VirtualList
      items={textSections}
      itemKey="key"
      estimatedItemHeight={58}
      itemGap={12}
      className={`${styles.body} scroll-shadow-bottom`}
      contentClassName={styles.bodyContent}
    >
      {(section) => section.kind === "intro"
        ? <div className={styles.intro}>
          <h2>{ad.content.title}</h2>
          <p>{ad.content.description}</p>
        </div>
        : <div className={styles.detail}>
          <h3>{section.label}</h3>
          <p>{section.value}</p>
        </div>}
    </VirtualList>
    <div className={styles.footer}>
      <button type="button" onClick={() => { onClose(); onAction(ad.content.button.action); }}>{ad.content.button.label}</button>
    </div>
  </div>, document.body);
}
