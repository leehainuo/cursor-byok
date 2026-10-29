import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
  type Ref,
} from "react"
import {
  useScrollableModel,
  type ScrollAreaApi,
  type ScrollAreaState,
} from "./useScrollableModel"
import styles from "./ScrollArea.module.scss"

export type { ScrollAreaApi, ScrollAreaState } from "./useScrollableModel"

export interface ScrollAreaProps {
  className?: string
  style?: CSSProperties
  viewportClassName?: string
  contentClassName?: string

  children: ReactNode | ((state: ScrollAreaState) => ReactNode)

  contentHeight?: number
  defaultScrollTop?: number
  scrollTop?: number
  onScroll?: (state: ScrollAreaState) => void
  onViewportResize?: (size: { width: number; height: number }) => void

  scrollbarSize?: number
  minThumbSize?: number
  scrollbarVisibility?: "auto" | "always" | "hidden"
  scrollbarInsetTop?: CSSProperties["top"]

  viewportRef?: Ref<HTMLDivElement>
  contentRef?: Ref<HTMLDivElement>
  onReady?: (api: ScrollAreaApi) => void
}

function assignRef<T>(ref: Ref<T> | undefined, value: T | null): void {
  if (!ref) return
  if (typeof ref === "function") {
    ref(value)
  } else {
    ref.current = value
  }
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max)
}

function normalizeWheelDelta(event: WheelEvent, viewportHeight: number): number {
  let deltaY = event.deltaY

  if (event.deltaMode === WheelEvent.DOM_DELTA_LINE) {
    deltaY *= 16
  } else if (event.deltaMode === WheelEvent.DOM_DELTA_PAGE) {
    deltaY *= viewportHeight
  }

  if (deltaY !== 0 && Math.abs(deltaY) < 1) {
    if (deltaY < 0) return -1
    return 1
  }

  return deltaY
}

export const ScrollArea = forwardRef<HTMLDivElement, ScrollAreaProps>(
  function ScrollArea(props, forwardedRef) {
    const {
      className,
      style,
      viewportClassName,
      contentClassName,
      children,
      contentHeight,
      defaultScrollTop,
      scrollTop,
      onScroll,
      onViewportResize,
      scrollbarSize = 8,
      minThumbSize = 20,
      scrollbarVisibility = "auto",
      scrollbarInsetTop = 0,
      viewportRef,
      contentRef,
      onReady,
    } = props

    const hostRef = useRef<HTMLDivElement | null>(null)
    const localViewportRef = useRef<HTMLDivElement | null>(null)
    const localContentRef = useRef<HTMLDivElement | null>(null)
    const trackRef = useRef<HTMLDivElement | null>(null)
    const thumbRef = useRef<HTMLDivElement | null>(null)
    const dragRef = useRef<{
      pointerId: number
      pointerOffsetY: number
    } | null>(null)
    const [dragging, setDragging] = useState(false)

    const { state, api, metricsRef, scrollToNow } = useScrollableModel({
      viewportRef: localViewportRef,
      contentRef: localContentRef,
      trackRef,
      thumbRef,
      contentHeight,
      defaultScrollTop,
      scrollTop,
      minThumbSize,
      onScroll,
      onViewportResize,
    })

    useImperativeHandle(forwardedRef, () => hostRef.current as HTMLDivElement)

    useEffect(() => {
      onReady?.(api)
    }, [api, onReady])

    const setViewportRef = useCallback(
      (node: HTMLDivElement | null) => {
        localViewportRef.current = node
        assignRef(viewportRef, node)
      },
      [viewportRef]
    )

    const setContentRef = useCallback(
      (node: HTMLDivElement | null) => {
        localContentRef.current = node
        assignRef(contentRef, node)
      },
      [contentRef]
    )

    const scrollbarHidden =
      scrollbarVisibility === "hidden" ||
      !state.isScrollable ||
      state.viewportHeight <= 0

    const hostClassNames = [styles.root]
    if (scrollbarVisibility === "always") hostClassNames.push(styles.always)
    if (dragging || state.isScrolling) hostClassNames.push(styles.active)
    if (className) hostClassNames.push(className)
    const hostClassName = hostClassNames.join(" ")

    const hostStyle = useMemo<CSSProperties>(
      () => ({
        ...style,
        "--oa-scrollbar-size": `${scrollbarSize}px`,
        "--oa-scrollbar-min-thumb-size": `${minThumbSize}px`,
        "--oa-scrollbar-inset-top":
          typeof scrollbarInsetTop === "number"
            ? `${scrollbarInsetTop}px`
            : scrollbarInsetTop,
      } as CSSProperties),
      [minThumbSize, scrollbarInsetTop, scrollbarSize, style]
    )

    useEffect(() => {
      const thumb = thumbRef.current
      const track = trackRef.current
      if (!thumb || !track) return

      const setScrollTopFromThumb = (clientY: number) => {
        const currentDrag = dragRef.current
        if (!currentDrag) return

        const metrics = metricsRef.current
        const trackRect = track.getBoundingClientRect()
        const desiredThumbTop = clamp(
          clientY - trackRect.top - currentDrag.pointerOffsetY,
          0,
          metrics.maxThumbTop
        )
        let nextScrollTop = 0
        if (metrics.maxThumbTop > 0) {
          nextScrollTop = (desiredThumbTop / metrics.maxThumbTop) * metrics.maxScrollTop
        }

        scrollToNow(nextScrollTop, true, metrics.maxScrollTop)
      }

      const stopDrag = (event: PointerEvent) => {
        if (dragRef.current?.pointerId !== event.pointerId) return
        if (thumb.hasPointerCapture(event.pointerId)) {
          thumb.releasePointerCapture(event.pointerId)
        }
        dragRef.current = null
        setDragging(false)
      }

      const handleThumbPointerDown = (event: PointerEvent) => {
        if (event.pointerType === "mouse" && event.button !== 0) return

        event.preventDefault()
        thumb.setPointerCapture(event.pointerId)

        const metrics = metricsRef.current
        const trackRect = track.getBoundingClientRect()
        dragRef.current = {
          pointerId: event.pointerId,
          pointerOffsetY: clamp(
            event.clientY - trackRect.top - metrics.thumbTop,
            0,
            metrics.thumbHeight
          ),
        }
        setDragging(true)
      }

      const handleThumbPointerMove = (event: PointerEvent) => {
        if (!dragRef.current || dragRef.current.pointerId !== event.pointerId) return
        event.preventDefault()
        setScrollTopFromThumb(event.clientY)
      }

      const handleTrackPointerDown = (event: PointerEvent) => {
        if (event.target !== track) return
        if (event.pointerType === "mouse" && event.button !== 0) return

        event.preventDefault()

        const rect = track.getBoundingClientRect()
        const metrics = metricsRef.current
        const desiredThumbTop = clamp(
          event.clientY - rect.top - metrics.thumbHeight / 2,
          0,
          metrics.maxThumbTop
        )
        let nextScrollTop = 0
        if (metrics.maxThumbTop > 0) {
          nextScrollTop = (desiredThumbTop / metrics.maxThumbTop) * metrics.maxScrollTop
        }
        scrollToNow(nextScrollTop, true, metrics.maxScrollTop)
      }

      thumb.addEventListener("pointerdown", handleThumbPointerDown)
      thumb.addEventListener("pointermove", handleThumbPointerMove)
      thumb.addEventListener("pointerup", stopDrag)
      thumb.addEventListener("pointercancel", stopDrag)
      thumb.addEventListener("lostpointercapture", stopDrag)
      track.addEventListener("pointerdown", handleTrackPointerDown)

      return () => {
        thumb.removeEventListener("pointerdown", handleThumbPointerDown)
        thumb.removeEventListener("pointermove", handleThumbPointerMove)
        thumb.removeEventListener("pointerup", stopDrag)
        thumb.removeEventListener("pointercancel", stopDrag)
        thumb.removeEventListener("lostpointercapture", stopDrag)
        track.removeEventListener("pointerdown", handleTrackPointerDown)
      }
    }, [metricsRef, scrollToNow])

    useEffect(() => {
      const host = hostRef.current
      const viewport = localViewportRef.current
      if (!host || !viewport) return

      const handleWheel = (event: WheelEvent) => {
        if (event.defaultPrevented || event.ctrlKey) return

        const metrics = metricsRef.current
        if (!metrics.isScrollable) return

        const deltaY = normalizeWheelDelta(event, metrics.viewportHeight)
        if (deltaY === 0) return

        event.preventDefault()
        event.stopPropagation()

        const currentTop = viewport.scrollTop
        const nextTop = clamp(currentTop + deltaY, 0, metrics.maxScrollTop)
        if (nextTop === currentTop) return

        scrollToNow(nextTop, true, metrics.maxScrollTop)
      }

      host.addEventListener("wheel", handleWheel, { passive: false })
      return () => {
        host.removeEventListener("wheel", handleWheel)
      }
    }, [metricsRef, scrollToNow])

    let renderedChildren: ReactNode = children as ReactNode
    if (typeof children === "function") renderedChildren = children(state)
    let scrollbarClassName = styles.scrollbar
    if (scrollbarHidden) scrollbarClassName = `${styles.scrollbar} ${styles.hidden}`

    return (
      <div
        ref={hostRef}
        className={hostClassName}
        style={hostStyle}
        data-scrolled={state.scrollTop > 0 ? "true" : "false"}
      >
        <div
          ref={setViewportRef}
          className={[styles.viewport, viewportClassName ?? ""]
            .filter(Boolean)
            .join(" ")}
        >
          <div ref={setContentRef} className={[styles.content, contentClassName ?? ""]
            .filter(Boolean)
            .join(" ")}
          >
            {renderedChildren}
          </div>
        </div>
        <div
          className={scrollbarClassName}
          aria-hidden="true"
        >
          <div ref={trackRef} className={styles.track} />
          <div
            ref={thumbRef}
            className={styles.thumb}
          />
        </div>
      </div>
    )
  }
)
