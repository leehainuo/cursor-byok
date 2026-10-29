import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react"
import {
  ScrollArea,
  type ScrollAreaApi,
  type ScrollAreaState,
} from "../scrollable/ScrollArea"
import { RangeMap } from "./rangeMap"
import type {
  VirtualListApi,
  VirtualListProps,
  VirtualListRenderContext,
} from "./virtualTypes"

interface RenderRange {
  start: number
  end: number
}

interface ViewportAnchor {
  index: number
  topDelta: number
}

interface ContentInsets {
  top: number
  bottom: number
}

interface VirtualRowProps {
  index: number
  top: number
  measure: (index: number, node: HTMLElement | null) => void
  children: ReactNode
}

function getVisibleRange(
  rangeMap: RangeMap,
  scrollTop: number,
  viewportHeight: number
): RenderRange {
  if (rangeMap.count === 0 || viewportHeight <= 0) return { start: 0, end: 0 }

  return {
    start: rangeMap.indexAt(scrollTop),
    end: rangeMap.indexAfter(scrollTop + viewportHeight - 1),
  }
}

function getRenderRange(
  rangeMap: RangeMap,
  scrollTop: number,
  viewportHeight: number,
  overscan: number
): RenderRange {
  const visibleRange = getVisibleRange(rangeMap, scrollTop, viewportHeight)

  return {
    start: Math.max(0, visibleRange.start - overscan),
    end: Math.min(rangeMap.count, visibleRange.end + overscan),
  }
}

function getViewportAnchor(
  rangeMap: RangeMap,
  scrollTop: number,
  viewportHeight: number
): ViewportAnchor | null {
  const visibleRange = getVisibleRange(rangeMap, scrollTop, viewportHeight)
  if (visibleRange.end <= visibleRange.start) return null

  if (scrollTop === rangeMap.positionAt(visibleRange.start)) {
    return {
      index: visibleRange.start,
      topDelta: 0,
    }
  }

  if (visibleRange.end - visibleRange.start > 1) {
    const index = visibleRange.start + 1
    return {
      index,
      topDelta: rangeMap.positionAt(index) - scrollTop,
    }
  }

  return null
}

function isSameRange(left: RenderRange, right: RenderRange): boolean {
  return left.start === right.start && left.end === right.end
}

function VirtualRow(props: VirtualRowProps) {
  const { index, top, measure, children } = props
  const rowRef = useRef<HTMLDivElement | null>(null)

  useLayoutEffect(() => {
    const node = rowRef.current
    if (!node) return

    measure(index, node)
    const resizeObserver = new ResizeObserver(() => {
      measure(index, node)
    })
    resizeObserver.observe(node)

    return () => {
      resizeObserver.disconnect()
      measure(index, null)
    }
  }, [index, measure])

  return (
    <div
      ref={rowRef}
      data-index={index}
      style={{
        left: 0,
        position: "absolute",
        top: 0,
        transform: `translateY(${top}px)`,
        width: "100%",
      }}
    >
      {children}
    </div>
  )
}

export function VirtualList<TItem>(props: VirtualListProps<TItem>) {
  const {
    items,
    getKey,
    estimateSize,
    renderItem,
    overscan = 0,
    itemGap = 0,
    deferInitialRender = false,
    scrollbarSize,
    minThumbSize,
    scrollbarVisibility,
    scrollbarInsetTop,
    className,
    contentClassName,
    style,
    onRangeChange,
    onReady,
  } = props

  const itemSnapshot = useMemo(
    () =>
      items.map((item, index) => {
        const key = getKey(item, index)
        const estimatedSize = estimateSize(item, index) + (index < items.length - 1 ? itemGap : 0)
        return { key, estimatedSize }
      }),
    [estimateSize, getKey, itemGap, items]
  )
  const rangeMapRef = useRef(
    new RangeMap(itemSnapshot.map((item) => item.estimatedSize))
  )
  const itemSignature = itemSnapshot
    .map((item) => `${item.key}:${item.estimatedSize}`)
    .join("\0")
  const itemSignatureRef = useRef(itemSignature)
  const shouldResetScrollRef = useRef(false)
  const scrollApiRef = useRef<ScrollAreaApi | null>(null)
  const scrollStateRef = useRef<ScrollAreaState | null>(null)
  const contentElementRef = useRef<HTMLDivElement | null>(null)
  const spacerRef = useRef<HTMLDivElement | null>(null)
  const [contentInsets, setContentInsets] = useState<ContentInsets>({
    top: 0,
    bottom: 0,
  })
  const renderRangeRef = useRef<RenderRange>({ start: 0, end: 0 })
  const [initialRenderReady, setInitialRenderReady] = useState(
    () => !deferInitialRender
  )
  const [scrollState, setScrollState] = useState<ScrollAreaState>({
    scrollTop: 0,
    viewportHeight: 0,
    scrollHeight: rangeMapRef.current.totalSize,
    isScrollable: false,
    isScrolling: false,
  })
  const [, forceUpdate] = useState(0)

  const readContentInsets = useCallback(() => {
    const node = contentElementRef.current
    const styles = node ? getComputedStyle(node) : null
    const nextInsets = {
      top: styles ? Number.parseFloat(styles.paddingTop) || 0 : 0,
      bottom: styles ? Number.parseFloat(styles.paddingBottom) || 0 : 0,
    }
    setContentInsets((currentInsets) =>
      currentInsets.top === nextInsets.top && currentInsets.bottom === nextInsets.bottom
        ? currentInsets
        : nextInsets
    )
  }, [])

  const setContentRef = useCallback((node: HTMLDivElement | null) => {
    contentElementRef.current = node
    readContentInsets()
  }, [readContentInsets])

  useLayoutEffect(() => {
    const node = contentElementRef.current
    if (!node) return

    readContentInsets()
    const resizeObserver = new ResizeObserver(readContentInsets)
    resizeObserver.observe(node)
    const frame = requestAnimationFrame(readContentInsets)

    return () => {
      cancelAnimationFrame(frame)
      resizeObserver.disconnect()
    }
  }, [readContentInsets])

  const contentInsetTop = contentInsets.top

  if (!scrollStateRef.current) {
    scrollStateRef.current = scrollState
  }

  useEffect(() => {
    if (!deferInitialRender) {
      setInitialRenderReady(true)
      return
    }

    let firstFrame = 0
    let secondFrame = 0
    firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        setInitialRenderReady(true)
      })
    })

    return () => {
      cancelAnimationFrame(firstFrame)
      cancelAnimationFrame(secondFrame)
    }
  }, [deferInitialRender])

  if (itemSignatureRef.current !== itemSignature) {
    const estimatedSizes = itemSnapshot.map((item) => item.estimatedSize)
    rangeMapRef.current.reset(estimatedSizes)
    itemSignatureRef.current = itemSignature
    shouldResetScrollRef.current = true
  }

  useEffect(() => {
    if (!shouldResetScrollRef.current) return
    shouldResetScrollRef.current = false
    scrollApiRef.current?.scrollToTop()
    forceUpdate((current) => current + 1)
  }, [itemSignature])

  const rangeMap = rangeMapRef.current
  const initialRenderCount = Math.max(1, overscan * 2 + 1)
  const canRenderItems =
    initialRenderReady &&
    (!deferInitialRender || scrollState.viewportHeight > 0)
  let renderRange: RenderRange
  if (!canRenderItems) {
    renderRange = { start: 0, end: 0 }
  } else if (scrollState.viewportHeight > 0) {
    renderRange = getRenderRange(
      rangeMap,
      Math.max(0, scrollState.scrollTop - contentInsetTop),
      scrollState.viewportHeight,
      overscan
    )
  } else {
    renderRange = {
      start: 0,
      end: Math.min(rangeMap.count, initialRenderCount),
    }
  }
  renderRangeRef.current = renderRange

  useEffect(() => {
    if (renderRange.end > renderRange.start) {
      onRangeChange?.({ start: renderRange.start, end: renderRange.end - 1 })
    }
  }, [onRangeChange, renderRange.end, renderRange.start])

  const measure = useCallback((index: number, node: HTMLElement | null) => {
    if (!node) return

    const rangeMap = rangeMapRef.current
    const currentScrollState = scrollStateRef.current
    let anchor: ViewportAnchor | null = null
    if (currentScrollState) {
      anchor = getViewportAnchor(
        rangeMap,
        Math.max(0, currentScrollState.scrollTop - contentInsetTop),
        currentScrollState.viewportHeight
      )
    }

    const measuredGap = index < items.length - 1 ? itemGap : 0
    const sizeDelta = rangeMap.updateSize(
      index,
      node.getBoundingClientRect().height + measuredGap
    )
    if (sizeDelta === 0) return

    if (spacerRef.current) {
      spacerRef.current.style.height = `${rangeMap.totalSize}px`
    }

    if (anchor && index < anchor.index) {
      scrollApiRef.current?.scrollTo(
        Math.max(0, contentInsetTop + rangeMap.positionAt(anchor.index) - anchor.topDelta)
      )
    }

    forceUpdate((current) => current + 1)
  }, [contentInsetTop, itemGap, items.length])

  const handleScroll = useCallback(
    (nextState: ScrollAreaState) => {
      const previousState = scrollStateRef.current
      scrollStateRef.current = nextState

      const nextRange = getRenderRange(
        rangeMapRef.current,
        Math.max(0, nextState.scrollTop - contentInsetTop),
        nextState.viewportHeight,
        overscan
      )
      const dimensionsChanged =
        !previousState ||
        previousState.viewportHeight !== nextState.viewportHeight ||
        previousState.scrollHeight !== nextState.scrollHeight
      const scrollingChanged =
        !previousState || previousState.isScrolling !== nextState.isScrolling
      const rangeChanged = !isSameRange(nextRange, renderRangeRef.current)

      if (
        dimensionsChanged ||
        scrollingChanged ||
        rangeChanged
      ) {
        setScrollState(nextState)
      }
    },
    [contentInsetTop, overscan]
  )

  const api = useMemo<VirtualListApi>(
    () => ({
      scrollToIndex(index, align = "auto") {
        const rangeMap = rangeMapRef.current
        const itemTop = contentInsetTop + rangeMap.positionAt(index)
        const itemHeight = rangeMap.sizeAt(index)
        const currentScrollState = scrollStateRef.current
        const viewportHeight = currentScrollState?.viewportHeight ?? 0
        const currentTop = currentScrollState?.scrollTop ?? 0
        const currentBottom = currentTop + viewportHeight

        let nextTop = itemTop
        if (align === "center") {
          nextTop = itemTop - (viewportHeight - itemHeight) / 2
        } else if (align === "end") {
          nextTop = itemTop - viewportHeight + itemHeight
        } else if (align === "auto") {
          if (itemTop >= currentTop && itemTop + itemHeight <= currentBottom) {
            return
          }
          if (itemTop < currentTop) nextTop = itemTop
          else nextTop = itemTop - viewportHeight + itemHeight
        }

        scrollApiRef.current?.scrollTo(nextTop)
      },
      scrollToOffset(offset) {
        scrollApiRef.current?.scrollTo(offset)
      },
      scrollToTop() {
        scrollApiRef.current?.scrollToTop()
      },
    }),
    [contentInsetTop]
  )

  useEffect(() => {
    onReady?.(api)
  }, [api, onReady])

  const visibleItems = []
  for (let index = renderRange.start; index < renderRange.end; index += 1) {
    visibleItems.push(index)
  }

  return (
    <ScrollArea
      className={className}
      contentClassName={contentClassName}
      contentRef={setContentRef}
      style={style}
      contentHeight={contentInsetTop + rangeMap.totalSize + contentInsets.bottom}
      onReady={(scrollApi) => {
        scrollApiRef.current = scrollApi
      }}
      onScroll={handleScroll}
      scrollbarSize={scrollbarSize}
      minThumbSize={minThumbSize}
      scrollbarVisibility={scrollbarVisibility}
      scrollbarInsetTop={scrollbarInsetTop}
    >
      <div
        ref={spacerRef}
        style={{
          height: `${rangeMap.totalSize}px`,
          position: "relative",
          width: "100%",
        }}
      >
        {visibleItems.map((index) => {
          const item = items[index]
          const context: VirtualListRenderContext = {
            index,
            isScrolling: scrollState.isScrolling,
            measureElement: (node) => measure(index, node),
          }

          return (
            <VirtualRow
              key={getKey(item, index)}
              index={index}
              top={rangeMap.positionAt(index)}
              measure={measure}
            >
              {renderItem(item, context)}
            </VirtualRow>
          )
        })}
      </div>
    </ScrollArea>
  )
}

export type {
  VirtualListApi,
  VirtualListProps,
  VirtualListRenderContext,
} from "./virtualTypes"
