import { BarChart, GaugeChart, LineChart } from "echarts/charts";
import { GridComponent, LegendComponent, MarkLineComponent, TooltipComponent } from "echarts/components";
import { getInstanceByDom, init, use, type EChartsCoreOption } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import { useEffect, useRef, type MouseEventHandler } from "react";
import styles from "./EChart.module.scss";

use([BarChart, GaugeChart, LineChart, GridComponent, LegendComponent, MarkLineComponent, TooltipComponent, CanvasRenderer]);

type EChartProps = {
  option: EChartsCoreOption;
  className?: string;
  onMouseEnter?: MouseEventHandler<HTMLDivElement>;
  onMouseLeave?: MouseEventHandler<HTMLDivElement>;
};

export function EChart({ option, className = styles.root, onMouseEnter, onMouseLeave }: EChartProps) {
  const element = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const node = element.current;
    if (!node) return;
    const chart = init(node, undefined, { renderer: "canvas" });
    const observer = new ResizeObserver(() => chart.resize());
    observer.observe(node);
    return () => {
      observer.disconnect();
      chart.dispose();
    };
  }, []);

  useEffect(() => {
    const node = element.current;
    if (node) getInstanceByDom(node)?.setOption(option, { notMerge: true });
  }, [option]);

  return <div ref={element} className={className} onMouseEnter={onMouseEnter} onMouseLeave={onMouseLeave} />;
}
