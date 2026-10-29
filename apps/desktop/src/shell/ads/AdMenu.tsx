import { useLayoutEffect, useRef, useState, type RefObject } from "react";
import { Marquee } from "react-css-marquee";
import { Icon } from "../../shared/ui/Icon";
import { windowCloseIcon } from "../../shared/ui/icons";
import type { AdSlot } from "./types";
import styles from "./Ads.module.scss";

type AdMenuProps = {
  ads: AdSlot[];
  activeAdId?: string;
  dismissingAdId?: string;
  readAdIds: ReadonlySet<string>;
  triggerRefs: RefObject<Map<string, HTMLButtonElement>>;
  onOpen: (ad: AdSlot) => void;
  onDismiss: (ad: AdSlot) => void;
};

type OverflowTextProps = {
  children: string;
  className: string;
};

function OverflowText({ children, className }: OverflowTextProps) {
  const container = useRef<HTMLDivElement>(null);
  const text = useRef<HTMLSpanElement>(null);
  const [overflowing, setOverflowing] = useState(false);

  useLayoutEffect(() => {
    const viewport = container.current;
    if (!viewport) return;

    const update = () => {
      const element = text.current;
      if (!element) return;
      setOverflowing(element.scrollWidth > viewport.clientWidth + 1);
    };

    update();
    const observer = new ResizeObserver(update);
    observer.observe(viewport);
    if (text.current) observer.observe(text.current);
    let disposed = false;
    void document.fonts?.ready.then(() => {
      if (!disposed) update();
    });
    return () => {
      disposed = true;
      observer.disconnect();
    };
  }, [children, overflowing]);

  return <div ref={container} className={styles.menuOverflow}>
    {overflowing
      ? <Marquee className={styles.menuMarquee} repeat loop={0} speed={12} gap={24}>
        <span ref={text} className={className}>{children}</span>
      </Marquee>
      : <span ref={text} className={className}>{children}</span>}
  </div>;
}

export function AdMenu({ ads, activeAdId, dismissingAdId, readAdIds, triggerRefs, onOpen, onDismiss }: AdMenuProps) {
  return <div className={styles.menuSlots} aria-label={t("推荐内容")}>
    {ads.map((ad) => <div key={ad.id} className={styles.menuSlotContainer}>
      <button
        ref={(node) => {
          if (node) triggerRefs.current.set(ad.id, node);
          else triggerRefs.current.delete(ad.id);
        }}
        type="button"
        className={styles.menuSlot}
        title={`${ad.target.title}\n${ad.target.description}`}
        aria-label={`${ad.target.title}，${ad.target.description}${readAdIds.has(ad.id) ? "" : `，${t("未读")}`}`}
        aria-haspopup="dialog"
        aria-expanded={activeAdId === ad.id}
        aria-controls={activeAdId === ad.id ? "menu-ad-dialog" : undefined}
        onClick={() => onOpen(ad)}
      >
        {!readAdIds.has(ad.id) && <span className={styles.unreadDot} aria-hidden="true" />}
        <img className={styles.menuImage} src={ad.target.imageUrl} alt="" />
        <div className={styles.menuCopy}>
          <OverflowText className={styles.menuTitle}>{ad.target.title}</OverflowText>
          <OverflowText className={styles.menuSubtitle}>{ad.target.description}</OverflowText>
        </div>
      </button>
      <button
        type="button"
        className={styles.menuDismiss}
        aria-label={`${t("不再显示广告")}：${ad.target.title}`}
        aria-haspopup="dialog"
        aria-expanded={dismissingAdId === ad.id}
        aria-controls={dismissingAdId === ad.id ? "dismiss-ad-dialog" : undefined}
        onClick={() => onDismiss(ad)}
      >
        <Icon icon={windowCloseIcon} size="1em" />
      </button>
    </div>)}
  </div>;
}
