import { useCallback, type CSSProperties, type ReactNode } from "react";
import { VirtualList as VirtualListEngine } from "./VirtualListEngine";
import type { VirtualListApi } from "./virtualTypes";

type ItemKey<TItem> = keyof TItem | ((item: TItem, index: number) => string | number);

export type VirtualListProps<TItem> = {
  items: TItem[];
  itemKey: ItemKey<TItem>;
  children: (item: TItem, index: number) => ReactNode;
  empty?: ReactNode;
  className?: string;
  contentClassName?: string;
  style?: CSSProperties;
  overscan?: number;
  itemGap?: number;
  estimatedItemHeight?: number;
  scrollbarInsetTop?: CSSProperties["top"];
  onReady?: (api: VirtualListApi) => void;
};

export function VirtualList<TItem>({
  items,
  itemKey,
  children,
  empty = null,
  className,
  contentClassName,
  style,
  overscan = 4,
  itemGap = 0,
  estimatedItemHeight = 54,
  scrollbarInsetTop,
  onReady,
}: VirtualListProps<TItem>) {
  const getKey = useCallback((item: TItem, index: number) => {
    const key = typeof itemKey === "function" ? itemKey(item, index) : item[itemKey];
    return String(key);
  }, [itemKey]);

  const estimateSize = useCallback(() => estimatedItemHeight, [estimatedItemHeight]);

  if (!items.length) {
    return <div className={className} style={style}>{empty}</div>;
  }

  return (
    <VirtualListEngine
      items={items}
      getKey={getKey}
      estimateSize={estimateSize}
      renderItem={(item, context) => children(item, context.index)}
      overscan={overscan}
      itemGap={itemGap}
      scrollbarSize={7}
      scrollbarInsetTop={scrollbarInsetTop}
      onReady={onReady}
      className={className}
      contentClassName={contentClassName}
      style={style}
    />
  );
}
