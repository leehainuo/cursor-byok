import type { EChartsCoreOption } from "echarts/core";
import { useEffect, useMemo, useState } from "react";
import { EChart } from "../charts/EChart";
import styles from "./CacheHitRateChart.module.scss";

const valueColor = "#40c463";
const trackColor = "rgba(139, 148, 158, 0.20)";

export function CacheHitRateChart({ rate, animationKey = 0 }: { rate: number; animationKey?: number }) {
  const finiteRate = Number.isFinite(rate) ? rate : 0;
  const percentage = Math.max(0, Math.min(100, finiteRate * 100));
  const [displayedPercentage, setDisplayedPercentage] = useState(0);
  const label = Number.isFinite(rate) ? `${percentage.toFixed(2)}%` : "--";

  useEffect(() => {
    setDisplayedPercentage(0);
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => setDisplayedPercentage(percentage));
    });
    return () => cancelAnimationFrame(frame);
  }, [animationKey, percentage]);

  const option = useMemo<EChartsCoreOption>(() => ({
    animationDuration: 0,
    animationDurationUpdate: displayedPercentage > 0 ? 1_000 : 0,
    animationEasing: "cubicOut",
    animationEasingUpdate: "cubicOut",
    series: [{
      type: "gauge",
      min: 0,
      max: 100,
      startAngle: 180,
      endAngle: 0,
      center: ["50%", "50%"],
      radius: "90%",
      silent: true,
      pointer: { show: false },
      progress: {
        show: true,
        roundCap: true,
        width: 11,
        itemStyle: { color: displayedPercentage > 0 ? valueColor : "transparent" },
      },
      axisLine: {
        roundCap: true,
        lineStyle: { width: 11, color: [[1, trackColor]] },
      },
      axisTick: { show: false },
      splitLine: { show: false },
      axisLabel: { show: false },
      anchor: { show: false },
      title: { show: false },
      detail: { show: false },
      data: [{ value: displayedPercentage }],
    }],
  }), [displayedPercentage]);

  return <div className={styles.root} role="img" aria-label={t("缓存命中率 {rate}", { rate: label })}>
    <EChart className={styles.chart} option={option} />
    <div className={styles.label}>{label}</div>
  </div>;
}
