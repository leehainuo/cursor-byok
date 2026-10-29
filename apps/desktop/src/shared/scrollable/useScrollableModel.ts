import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react"

export interface ScrollAreaApi {
  scrollTo(top: number): void
  scrollBy(deltaTop: number): void
  scrollToTop(): void
  scrollToBottom(): void
  getScrollElement(): HTMLDivElement | null
}

export interface ScrollAreaState {
  scrollTop: number
  viewportHeight: number
  scrollHeight: number
  isScrollable: boolean
  isScrolling: boolean
}

export interface ScrollMetrics extends ScrollAreaState {
  maxScrollTop: number
  trackHeight: number
  thumbHeight: number
  thumbTop: number
  maxThumbTop: number
}

interface UseScrollableModelOptions {
  viewportRef: RefObject<HTMLDivElement | null>
  contentRef: RefObject<HTMLDivElement | null>
  trackRef: RefObject<HTMLDivElement | null>
  thumbRef: RefObject<HTMLDivElement | null>
  contentHeight?: number
  defaultScrollTop?: number
  scrollTop?: number
  minThumbSize?: number
  onScroll?: (state: ScrollAreaState) => void
  onViewportResize?: (size: { width: number; height: number }) => void
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max)
}

function readScrollHeight(
  element: HTMLDivElement | null,
  contentHeight?: number
): number {
  if (typeof contentHeight === "number") return Math.max(0, contentHeight)
  if (!element) return contentHeight ?? 0
  return element.scrollHeight
}

function measureScrollArea(
  element: HTMLDivElement | null,
  track: HTMLDivElement | null,
  contentHeight: number | undefined,
  minThumbSize: number,
  isScrolling: boolean
): ScrollMetrics {
  const scrollHeight = readScrollHeight(element, contentHeight)
  const viewportHeight = element?.clientHeight ?? 0
  const trackHeight = track?.clientHeight ?? viewportHeight
  const maxScrollTop = Math.max(0, scrollHeight - viewportHeight)
  const scrollTop = clamp(element?.scrollTop ?? 0, 0, maxScrollTop)
  const isScrollable = scrollHeight > viewportHeight + 1

  let rawThumbHeight = 0
  if (scrollHeight > 0) {
    rawThumbHeight = (trackHeight * viewportHeight) / scrollHeight
  }
  let thumbHeight = 0
  if (isScrollable && trackHeight > 0) {
    thumbHeight = Math.min(trackHeight, Math.max(minThumbSize, rawThumbHeight))
  }
  const maxThumbTop = Math.max(0, trackHeight - thumbHeight)
  let thumbTop = 0
  if (maxScrollTop > 0) thumbTop = (scrollTop / maxScrollTop) * maxThumbTop

  return {
    scrollTop,
    viewportHeight,
    scrollHeight,
    isScrollable,
    isScrolling,
    maxScrollTop,
    trackHeight,
    thumbHeight,
    thumbTop,
    maxThumbTop,
  }
}

function toState(metrics: ScrollMetrics): ScrollAreaState {
  return {
    scrollTop: metrics.scrollTop,
    viewportHeight: metrics.viewportHeight,
    scrollHeight: metrics.scrollHeight,
    isScrollable: metrics.isScrollable,
    isScrolling: metrics.isScrolling,
  }
}

function shouldCommitState(
  previous: ScrollAreaState,
  next: ScrollAreaState
): boolean {
  return (
    previous.viewportHeight !== next.viewportHeight ||
    previous.scrollHeight !== next.scrollHeight ||
    previous.isScrollable !== next.isScrollable ||
    previous.isScrolling !== next.isScrolling ||
    (previous.scrollTop > 0) !== (next.scrollTop > 0)
  )
}

function applyThumbStyle(
  thumb: HTMLDivElement | null,
  metrics: ScrollMetrics
): void {
  if (!thumb) return
  thumb.style.height = `${metrics.thumbHeight}px`
  thumb.style.transform = `translateY(${metrics.thumbTop}px)`
}

export function useScrollableModel(options: UseScrollableModelOptions) {
  const {
    viewportRef,
    contentRef,
    trackRef,
    thumbRef,
    contentHeight,
    defaultScrollTop = 0,
    scrollTop,
    minThumbSize = 20,
    onScroll,
    onViewportResize,
  } = options

  const [state, setState] = useState<ScrollAreaState>(() =>
    toState(measureScrollArea(null, null, contentHeight, minThumbSize, false))
  )
  const stateRef = useRef(state)
  const reactStateRef = useRef(state)
  const metricsRef = useRef<ScrollMetrics>(
    measureScrollArea(null, null, contentHeight, minThumbSize, false)
  )
  const scrollEndTimerRef = useRef<number | null>(null)
  const programmaticScrollTopRef = useRef<number | null>(null)
  const pendingNotificationRef = useRef<ScrollAreaState | null>(null)
  const notificationScheduledRef = useRef(false)
  const mountedRef = useRef(true)
  const defaultScrollTopAppliedRef = useRef(false)
  const onScrollRef = useRef(onScroll)
  const onViewportResizeRef = useRef(onViewportResize)

  onScrollRef.current = onScroll
  onViewportResizeRef.current = onViewportResize

  const scheduleNotification = useCallback((nextState: ScrollAreaState) => {
    pendingNotificationRef.current = nextState
    if (notificationScheduledRef.current) return

    notificationScheduledRef.current = true
    queueMicrotask(() => {
      notificationScheduledRef.current = false
      const pendingState = pendingNotificationRef.current
      pendingNotificationRef.current = null
      if (!pendingState || !mountedRef.current) return

      if (shouldCommitState(reactStateRef.current, pendingState)) {
        reactStateRef.current = pendingState
        setState(pendingState)
      }
      onScrollRef.current?.(pendingState)
    })
  }, [])

  const snapshot = useCallback(
    (isScrolling = false): ScrollMetrics => {
      const metrics = measureScrollArea(
        viewportRef.current,
        trackRef.current,
        contentHeight,
        minThumbSize,
        isScrolling
      )
      metricsRef.current = metrics
      applyThumbStyle(thumbRef.current, metrics)

      const nextState = toState(metrics)
      stateRef.current = nextState
      scheduleNotification(nextState)

      return metrics
    },
    [contentHeight, minThumbSize, scheduleNotification, thumbRef, trackRef, viewportRef]
  )

  const scheduleScrollEnd = useCallback(() => {
    if (scrollEndTimerRef.current !== null) {
      window.clearTimeout(scrollEndTimerRef.current)
    }

    scrollEndTimerRef.current = window.setTimeout(() => {
      scrollEndTimerRef.current = null
      snapshot(false)
    }, 140)
  }, [snapshot])

  const scrollToNow = useCallback(
    (
      top: number,
      isScrolling = true,
      knownMaxScrollTop?: number
    ): ScrollMetrics | null => {
      const element = viewportRef.current
      if (!element) return null

      let maxScrollTop = knownMaxScrollTop
      if (typeof maxScrollTop !== "number") {
        maxScrollTop = measureScrollArea(
          element,
          trackRef.current,
          contentHeight,
          minThumbSize,
          isScrolling
        ).maxScrollTop
      }
      const nextTop = clamp(top, 0, maxScrollTop)

      if (element.scrollTop !== nextTop) {
        programmaticScrollTopRef.current = nextTop
        element.scrollTop = nextTop
      }

      const nextMetrics = snapshot(isScrolling)
      if (isScrolling) scheduleScrollEnd()
      return nextMetrics
    },
    [contentHeight, minThumbSize, scheduleScrollEnd, snapshot, trackRef, viewportRef]
  )

  const api = useMemo<ScrollAreaApi>(
    () => ({
      scrollTo(top: number) {
        scrollToNow(top)
      },
      scrollBy(deltaTop: number) {
        const element = viewportRef.current
        if (!element) return
        scrollToNow(element.scrollTop + deltaTop)
      },
      scrollToTop() {
        scrollToNow(0)
      },
      scrollToBottom() {
        const element = viewportRef.current
        if (!element) return
        const metrics = measureScrollArea(
          element,
          trackRef.current,
          contentHeight,
          minThumbSize,
          true
        )
        scrollToNow(metrics.maxScrollTop)
      },
      getScrollElement() {
        return viewportRef.current
      },
    }),
    [contentHeight, minThumbSize, scrollToNow, trackRef, viewportRef]
  )

  useEffect(() => {
    const element = viewportRef.current
    if (!element || defaultScrollTopAppliedRef.current) return

    defaultScrollTopAppliedRef.current = true
    element.scrollTop = defaultScrollTop
    snapshot(false)
  }, [defaultScrollTop, snapshot, viewportRef])

  useEffect(() => {
    if (typeof scrollTop !== "number") return
    const element = viewportRef.current
    if (!element || element.scrollTop === scrollTop) return
    element.scrollTop = scrollTop
    snapshot(false)
  }, [scrollTop, snapshot, viewportRef])

  useEffect(() => {
    const element = viewportRef.current
    if (!element) return

    const handleScroll = () => {
      if (
        programmaticScrollTopRef.current !== null &&
        Math.abs(element.scrollTop - programmaticScrollTopRef.current) < 0.5
      ) {
        programmaticScrollTopRef.current = null
        return
      }

      snapshot(true)
      scheduleScrollEnd()
    }

    element.addEventListener("scroll", handleScroll, { passive: true })
    return () => {
      element.removeEventListener("scroll", handleScroll)
    }
  }, [scheduleScrollEnd, snapshot, viewportRef])

  useEffect(() => {
    const element = viewportRef.current
    const content = contentRef.current
    const track = trackRef.current
    if (!element || !content || !track) return

    const resizeObserver = new ResizeObserver((entries) => {
      const viewportEntry = entries.find((entry) => entry.target === element)
      if (viewportEntry) {
        onViewportResizeRef.current?.({
          width: viewportEntry.contentRect.width,
          height: viewportEntry.contentRect.height,
        })
      }
      snapshot(false)
    })

    resizeObserver.observe(element)
    resizeObserver.observe(content)
    resizeObserver.observe(track)
    snapshot(false)

    return () => {
      resizeObserver.disconnect()
    }
  }, [contentRef, snapshot, trackRef, viewportRef])

  useEffect(() => {
    snapshot(false)
  }, [contentHeight, snapshot])

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      pendingNotificationRef.current = null
      notificationScheduledRef.current = false
      if (scrollEndTimerRef.current !== null) {
        window.clearTimeout(scrollEndTimerRef.current)
      }
    }
  }, [])

  return { state, api, metricsRef, snapshot, scrollToNow }
}
