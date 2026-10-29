import type { CSSProperties, ReactNode } from "react"

export interface VirtualListApi {
  scrollToIndex(
    index: number,
    align?: "start" | "center" | "end" | "auto"
  ): void
  scrollToOffset(offset: number): void
  scrollToTop(): void
}

export interface VirtualListRenderContext {
  index: number
  measureElement: (node: HTMLElement | null) => void
  isScrolling: boolean
}

export interface VirtualListProps<TItem> {
  items: TItem[]
  getKey: (item: TItem, index: number) => string
  estimateSize: (item: TItem, index: number) => number
  renderItem: (item: TItem, context: VirtualListRenderContext) => ReactNode

  overscan?: number
  itemGap?: number
  deferInitialRender?: boolean
  scrollbarSize?: number
  minThumbSize?: number
  scrollbarVisibility?: "auto" | "always" | "hidden"
  scrollbarInsetTop?: CSSProperties["top"]
  className?: string
  contentClassName?: string
  style?: CSSProperties
  onRangeChange?: (range: { start: number; end: number }) => void
  onReady?: (api: VirtualListApi) => void
}

export interface VirtualTreeNode {
  id: string
  parentId?: string
  depth: number
  collapsible?: boolean
  disabled?: boolean
}

export interface VirtualTreeRenderContext {
  index: number
  depth: number
  expanded: boolean
  selected: boolean
  toggleExpanded(): void
  measureElement: (node: HTMLElement | null) => void
}

export interface VirtualTreeProps<TNode extends VirtualTreeNode> {
  nodes: TNode[]
  getKey?: (node: TNode) => string
  estimateSize: (node: TNode, index: number) => number
  renderNode: (node: TNode, context: VirtualTreeRenderContext) => ReactNode

  expandedIds?: Set<string>
  defaultExpandedIds?: Set<string>
  onExpandedChange?: (expandedIds: Set<string>) => void

  selectedId?: string
  onSelect?: (node: TNode) => void

  overscan?: number
  scrollbarSize?: number
  minThumbSize?: number
  scrollbarVisibility?: "auto" | "always" | "hidden"
  className?: string
  style?: CSSProperties
}

export interface VirtualComboboxOption {
  key: string
  label: string
  description?: string
  searchText?: string
}

export interface VirtualComboboxProps {
  options: VirtualComboboxOption[]
  value: string
  placeholder: string
  emptyText: string
  ariaLabel?: string
  onChange: (key: string) => void
  className?: string
  estimateSize?: number
  maxPopupHeight?: number
  scrollbarSize?: number
  minThumbSize?: number
  scrollbarVisibility?: "auto" | "always" | "hidden"
}
