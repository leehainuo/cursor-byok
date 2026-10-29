import { useId, useState, type KeyboardEvent, type ReactNode } from "react";
import styles from "./Tabs.module.scss";

export type TabItem = {
  value: string;
  label: string;
  content: ReactNode;
};

export function Tabs({ items, defaultValue }: { items: TabItem[]; defaultValue?: string }) {
  const initialValue = defaultValue && items.some((item) => item.value === defaultValue)
    ? defaultValue
    : items[0]?.value;
  const [activeValue, setActiveValue] = useState(initialValue);
  const tabsId = useId();
  const activeItem = items.find((item) => item.value === activeValue) ?? items[0];

  if (!activeItem) return null;

  const selectAdjacent = (event: KeyboardEvent<HTMLButtonElement>, offset: number) => {
    const currentIndex = items.findIndex((item) => item.value === activeItem.value);
    const nextIndex = (currentIndex + offset + items.length) % items.length;
    setActiveValue(items[nextIndex].value);
    event.currentTarget.parentElement
      ?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[nextIndex]
      ?.focus();
  };

  return <div className={styles.root}>
    <div className={styles.list} role="tablist">
      {items.map((item) => {
        const selected = item.value === activeItem.value;
        return <button
          key={item.value}
          id={`${tabsId}-tab-${item.value}`}
          className={styles.tab}
          type="button"
          role="tab"
          aria-selected={selected}
          aria-controls={`${tabsId}-panel-${item.value}`}
          tabIndex={selected ? 0 : -1}
          onClick={() => setActiveValue(item.value)}
          onKeyDown={(event) => {
            if (event.key === "ArrowLeft") selectAdjacent(event, -1);
            if (event.key === "ArrowRight") selectAdjacent(event, 1);
          }}
        >{item.label}</button>;
      })}
    </div>
    <div
      id={`${tabsId}-panel-${activeItem.value}`}
      className={styles.panel}
      role="tabpanel"
      aria-labelledby={`${tabsId}-tab-${activeItem.value}`}
    >{activeItem.content}</div>
  </div>;
}
