import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { init, Rect, type ElementEvent } from "zrender";
import type { Locale } from "../../../i18n/runtime";
import { useI18n } from "../../../i18n/store";
import { useTooltip, type TooltipAnchor } from "../../../shared/ui/Tooltip";
import styles from "./ContributionCalendarChart.module.scss";

export type ContributionDay = {
  date: string;
  tokens: number;
};

type ContributionCalendarChartProps = {
  data: ContributionDay[];
};

type CalendarCell = ContributionDay & {
  column: number;
  row: number;
  level: number;
};

type CellExtra = CalendarCell & {
  kind: "calendar-cell";
  x: number;
  y: number;
  width: number;
  height: number;
};

type AxisLabel = {
  key: string;
  text: string;
  left: number;
};

const levelColors = [
  "rgba(139, 148, 158, 0.20)",
  "#9be9a8",
  "#40c463",
  "#30a14e",
  "#216e39",
];
const DAY_IN_MS = 24 * 60 * 60 * 1000;
const CALENDAR_CONFIG = {
  cellAspectRatio: 0.9,
  cellGap: 3,
  resizeTransitionMs: 180,
  rowCount: 7,
  axisLabelGap: 8,
  axisLabelWidth: 28,
} as const;
function parseDate(date: string) {
  return new Date(`${date}T00:00:00Z`);
}

function mondayIndex(date: Date) {
  return (date.getUTCDay() + 6) % 7;
}

function cellOffset(index: number, cellSize: number) {
  return index * (cellSize + CALENDAR_CONFIG.cellGap);
}

function isCellExtra(value: unknown): value is CellExtra {
  return typeof value === "object" && value !== null && (value as CellExtra).kind === "calendar-cell";
}

function buildCalendarLayout(data: ContributionDay[], locale: Locale) {
  if (data.length === 0) return null;

  const monthFormatter = new Intl.DateTimeFormat(locale, { month: "short", timeZone: "UTC" });
  const maximum = Math.max(1, ...data.map(({ tokens }) => tokens));
  const firstDate = parseDate(data[0].date);
  const calendarStart = new Date(firstDate.getTime() - mondayIndex(firstDate) * DAY_IN_MS);
  const cells: CalendarCell[] = data.map((day) => {
    const date = parseDate(day.date);
    const daysFromStart = Math.round((date.getTime() - calendarStart.getTime()) / DAY_IN_MS);
    const level = day.tokens === 0 ? 0 : Math.max(1, Math.ceil((day.tokens / maximum) * 4));
    return { ...day, column: Math.floor(daysFromStart / 7), row: mondayIndex(date), level };
  });
  const columnCount = cells.at(-1)!.column + 1;
  const monthTicks = cells.reduce<Array<{ key: string; text: string; column: number }>>((ticks, cell) => {
    const date = parseDate(cell.date);
    const key = `${date.getUTCFullYear()}-${date.getUTCMonth()}`;
    if (ticks.at(-1)?.key !== key) ticks.push({ key, text: monthFormatter.format(date), column: cell.column });
    return ticks;
  }, []);
  return { cells, columnCount, monthTicks };
}

export function ContributionCalendarChart({ data }: ContributionCalendarChartProps) {
  const { locale } = useI18n();
  const scrollerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLDivElement>(null);
  const layoutRef = useRef<ReturnType<typeof buildCalendarLayout>>(null);
  const scheduleDrawRef = useRef<() => void>(() => undefined);
  const { show: showTooltip, hide: hideTooltip } = useTooltip();
  const [axisLabels, setAxisLabels] = useState<AxisLabel[]>([]);
  const layout = useMemo(() => buildCalendarLayout(data, locale), [data, locale]);
  const tokenFormatter = useMemo(() => new Intl.NumberFormat(locale), [locale]);
  layoutRef.current = layout;

  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    const node = canvasRef.current;
    if (!scroller || !node) return;

    const chart = init(node, {
      renderer: "canvas",
      width: 1,
      height: 1,
      useDirtyRect: true,
    });

    const handleMouseOver = (event: ElementEvent) => {
      const extra = event.target?.extra;
      if (!isCellExtra(extra)) return;
      const anchor: TooltipAnchor = {
        contextElement: node,
        getBoundingClientRect: () => {
          const bounds = node.getBoundingClientRect();
          return new DOMRect(bounds.left + extra.x, bounds.top + extra.y, extra.width, extra.height);
        },
      };
      showTooltip(anchor, undefined, <div className={styles.tooltipContent}>
        <strong>{extra.date}</strong>
        <span>{t("Token 用量：{tokens}", { tokens: tokenFormatter.format(extra.tokens) })}</span>
      </div>);
    };
    const handleMouseOut = (event: ElementEvent) => {
      if (isCellExtra(event.target?.extra)) hideTooltip();
    };

    chart.on("mouseover", handleMouseOver);
    chart.on("mouseout", handleMouseOut);

    const cellRects = new Map<string, Rect>();
    let drawFrame = 0;
    let lastAvailableWidth = -1;
    let lastCanvasHeight = -1;
    let lastLayout: typeof layout = null;
    const draw = () => {
      const currentLayout = layoutRef.current;
      if (!currentLayout) return;
      const availableWidth = Math.floor(scroller.getBoundingClientRect().width);
      if (availableWidth <= 0 || (availableWidth === lastAvailableWidth && currentLayout === lastLayout)) return;
      const gapsWidth = (currentLayout.columnCount - 1) * CALENDAR_CONFIG.cellGap;
      const cellWidth = Math.max(0, (availableWidth - gapsWidth) / currentLayout.columnCount);
      const cellHeight = cellWidth / CALENDAR_CONFIG.cellAspectRatio;
      const width = availableWidth;
      const height = CALENDAR_CONFIG.rowCount * cellHeight
        + (CALENDAR_CONFIG.rowCount - 1) * CALENDAR_CONFIG.cellGap;

      let lastLabelEnd = -Infinity;
      const nextAxisLabels = currentLayout.monthTicks.flatMap((tick) => {
        const left = Math.min(
          cellOffset(tick.column, cellWidth),
          availableWidth - CALENDAR_CONFIG.axisLabelWidth,
        );
        if (left < lastLabelEnd + CALENDAR_CONFIG.axisLabelGap) return [];
        lastLabelEnd = left + CALENDAR_CONFIG.axisLabelWidth;
        return [{ ...tick, left }];
      });
      setAxisLabels(nextAxisLabels);

      if (availableWidth !== lastAvailableWidth || height !== lastCanvasHeight) {
        node.style.width = "100%";
        node.style.height = `${height}px`;
        chart.resize({ width, height });
        lastAvailableWidth = availableWidth;
        lastCanvasHeight = height;
      }
      lastLayout = currentLayout;

      const currentDates = new Set(currentLayout.cells.map((cell) => cell.date));
      for (const [date, rect] of cellRects) {
        if (currentDates.has(date)) continue;
        chart.remove(rect);
        cellRects.delete(date);
      }

      for (const cell of currentLayout.cells) {
        const x = cellOffset(cell.column, cellWidth);
        const y = cellOffset(cell.row, cellHeight);
        const shape = {
          x,
          y,
          width: cellWidth,
          height: cellHeight,
          r: Math.min(3, Math.min(cellWidth, cellHeight) / 4),
        };
        const extra = {
          ...cell,
          kind: "calendar-cell" as const,
          x,
          y,
          width: cellWidth,
          height: cellHeight,
        } satisfies CellExtra;
        const current = cellRects.get(cell.date);

        if (current) {
          current.extra = extra;
          current.stopAnimation();
          current.animateTo(
            { shape, style: { fill: levelColors[cell.level] } },
            { duration: CALENDAR_CONFIG.resizeTransitionMs, easing: "cubicOut" },
          );
          continue;
        }

        const rect = new Rect({
          shape,
          style: {
            fill: levelColors[cell.level],
            stroke: "rgba(139, 148, 158, 0.10)",
            lineWidth: 1,
          },
          cursor: "default",
          extra,
        });
        cellRects.set(cell.date, rect);
        chart.add(rect);
      }
      hideTooltip();
    };
    const scheduleDraw = () => {
      window.cancelAnimationFrame(drawFrame);
      drawFrame = window.requestAnimationFrame(draw);
    };
    scheduleDrawRef.current = scheduleDraw;
    const observer = new ResizeObserver(scheduleDraw);
    observer.observe(scroller);
    scheduleDraw();

    return () => {
      observer.disconnect();
      window.cancelAnimationFrame(drawFrame);
      scheduleDrawRef.current = () => undefined;
      chart.dispose();
    };
  }, [layout !== null]);

  useLayoutEffect(() => {
    scheduleDrawRef.current();
  }, [layout]);

  if (!layout) return null;

  return (
    <section className={styles.root} aria-label={t("过去一年的 Token 用量")}>
      <div ref={scrollerRef} className={styles.scroller}>
        <div
          ref={canvasRef}
          className={styles.canvas}
          role="img"
          aria-label={t("过去一年的 Token 用量日历")}
        />
        <div className={styles.axis} aria-hidden="true">
          {axisLabels.map((label) => <span key={label.key} style={{ left: label.left }}>{label.text}</span>)}
        </div>
      </div>
    </section>
  );
}
