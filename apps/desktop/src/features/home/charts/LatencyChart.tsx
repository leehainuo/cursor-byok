import type { EChartsCoreOption } from "echarts/core";
import type { LlmCall } from "../../../shared/api";
import { EChart } from "./EChart";

export function LatencyChart({ calls }: { calls: LlmCall[] }) {
  const points = calls.slice(0, 20).reverse();
  const option: EChartsCoreOption = {
    animationDuration: 280,
    color: ["#d79a62", "#ad84cf", "#72a8d8"],
    grid: { top: 22, right: 18, bottom: 30, left: 48 },
    legend: { top: 0, right: 8, textStyle: { color: "#999" } },
    tooltip: { trigger: "axis" },
    xAxis: { type: "category", data: points.map((call) => new Date(call.created_at_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })), axisLabel: { color: "#888" }, axisLine: { lineStyle: { color: "#5555" } } },
    yAxis: { type: "value", axisLabel: { color: "#888", formatter: "{value} ms" }, splitLine: { lineStyle: { color: "#8882" } } },
    series: [
      { name: "TTFR", type: "line", smooth: true, symbol: "none", data: points.map((call) => call.ttfr_ms ?? 0) },
      { name: "TTFT", type: "line", smooth: true, symbol: "none", data: points.map((call) => call.ttft_ms ?? 0) },
      { name: t("总耗时"), type: "line", smooth: true, symbol: "none", data: points.map((call) => call.duration_ms ?? 0) },
    ],
  };
  return <EChart option={option} />;
}
