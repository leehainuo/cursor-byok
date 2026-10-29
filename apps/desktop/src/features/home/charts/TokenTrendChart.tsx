import type { EChartsCoreOption } from "echarts/core";
import type { LlmCall } from "../../../shared/api";
import { EChart } from "./EChart";

export function TokenTrendChart({ calls }: { calls: LlmCall[] }) {
  const points = calls.slice(0, 20).reverse();
  const option: EChartsCoreOption = {
    animationDuration: 280,
    color: ["#6f91f4", "#62c6a3"],
    grid: { top: 22, right: 18, bottom: 30, left: 48 },
    legend: { top: 0, right: 8, textStyle: { color: "#999" } },
    tooltip: { trigger: "axis" },
    xAxis: { type: "category", data: points.map((call) => new Date(call.created_at_ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })), axisLabel: { color: "#888" }, axisLine: { lineStyle: { color: "#5555" } } },
    yAxis: { type: "value", axisLabel: { color: "#888" }, splitLine: { lineStyle: { color: "#8882" } } },
    series: [
      { name: t("输入 Token"), type: "bar", stack: "tokens", data: points.map((call) => call.input_tokens ?? 0), barMaxWidth: 22 },
      { name: t("输出 Token"), type: "bar", stack: "tokens", data: points.map((call) => call.output_tokens ?? 0), barMaxWidth: 22 },
    ],
  };
  return <EChart option={option} />;
}
