import { useCallback, type ReactNode } from "react";
import { VirtualList } from "../../shared/virtual/VirtualListEngine";
import styles from "./VirtualPage.module.scss";

export type VirtualPageSection = {
  key: string;
  estimatedHeight: number;
  content: ReactNode;
};

export function VirtualPage({ title, sections, className, contentClassName }: { title?: ReactNode; sections: VirtualPageSection[]; className?: string; contentClassName?: string }) {
  const getKey = useCallback((section: VirtualPageSection) => section.key, []);
  const estimateSize = useCallback((section: VirtualPageSection) => section.estimatedHeight, []);
  const renderItem = useCallback((section: VirtualPageSection, { index }: { index: number }) => <section className={styles.section}>
    {index === 0 && title != null && <div className={styles.title}>{title}</div>}
    {section.content}
  </section>, [title]);

  return <VirtualList
    items={sections}
    getKey={getKey}
    estimateSize={estimateSize}
    renderItem={renderItem}
    overscan={2}
    itemGap={16}
    scrollbarSize={7}
    scrollbarInsetTop="var(--app-content-top)"
    className={[styles.root, "scroll-shadow-top", className].filter(Boolean).join(" ")}
    contentClassName={[styles.content, contentClassName].filter(Boolean).join(" ")}
  />;
}
