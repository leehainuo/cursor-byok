import { autoUpdate, computePosition, flip, offset, shift, size } from "@floating-ui/dom";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { parseTimeInput } from "../../../shared/utils/parseTimeInput";
import controls from "../../../shared/ui/Controls.module.scss";
import { Icon } from "../../../shared/ui/Icon";
import { ModelSelect, type ModelSelectOption } from "../../../shared/ui/ModelSelect";
import { TooltipTrigger } from "../../../shared/ui/TooltipTrigger";
import { refreshIcon } from "../../../shared/ui/icons";
import styles from "./OverviewTimeRangeFilter.module.scss";

export type OverviewRangePreset = "ten-minutes" | "hour" | "today" | "week" | "month" | "custom";
export type QuickPreset = "four-hours" | "twenty-four-hours";

export function OverviewTimeRangeFilter({ value, quick, customOpen, customStart, customEnd, modelOptions, selectedModels, busy, onSelect, onQuickSelect, onCustomOpenChange, onCustomStartChange, onCustomEndChange, onSelectedModelsChange, onCustomApply, onRefresh }: {
  value: OverviewRangePreset;
  quick: QuickPreset | null;
  customOpen: boolean;
  customStart: string;
  customEnd: string;
  modelOptions: ModelSelectOption[];
  selectedModels: string[];
  busy: boolean;
  onSelect: (value: Exclude<OverviewRangePreset, "custom">) => void;
  onQuickSelect: (durationMs: number) => void;
  onCustomOpenChange: (open: boolean) => void;
  onCustomStartChange: (value: string) => void;
  onCustomEndChange: (value: string) => void;
  onSelectedModelsChange: (value: string[]) => void;
  onCustomApply: () => void;
  onRefresh: () => void;
}) {
  const presets: Array<{ value: Exclude<OverviewRangePreset, "custom">; label: string }> = [
    { value: "hour", label: t("近1小时") },
    { value: "today", label: t("近1自然日") },
    { value: "ten-minutes", label: t("近10分钟") },
    { value: "week", label: t("近一周") },
    { value: "month", label: t("近一个月") },
  ];
  const quickPresets: Array<{ value: QuickPreset; label: string }> = [
    { value: "four-hours", label: t("近4小时") },
    { value: "twenty-four-hours", label: t("近24小时") },
  ];
  const customButton = useRef<HTMLButtonElement>(null);
  const popover = useRef<HTMLDivElement>(null);
  const popoverId = useId();
  const [position, setPosition] = useState({ left: 0, top: 0, width: 300, maxHeight: 480 });

  useLayoutEffect(() => {
    if (!customOpen || !customButton.current || !popover.current) return;
    return autoUpdate(customButton.current, popover.current, () => void computePosition(customButton.current!, popover.current!, {
      placement: "bottom-end",
      middleware: [offset(5), flip({ padding: 10 }), shift({ padding: 10 }), size({
        padding: 10,
        apply: ({ availableHeight }) => setPosition((current) => ({
          ...current,
          maxHeight: Math.max(240, availableHeight),
        })),
      })],
    }).then(({ x, y }) => setPosition((current) => ({ ...current, left: x, top: y }))));
  }, [customOpen]);

  useEffect(() => {
    if (!customOpen) return;
    const closeOutside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!customButton.current?.contains(target) && !popover.current?.contains(target)) onCustomOpenChange(false);
    };
    document.addEventListener("pointerdown", closeOutside);
    return () => document.removeEventListener("pointerdown", closeOutside);
  }, [customOpen, onCustomOpenChange]);

  const parsedStart = parseTimeInput(customStart);
  const parsedEnd = parseTimeInput(customEnd);
  const customValid = parsedStart !== null && parsedEnd !== null && parsedStart < parsedEnd;
  return <div className={styles.root} aria-label={t("概览时间范围")}>
    <div className={styles.presets}>
      {presets.map((preset) => <button
        key={preset.value}
        type="button"
        aria-pressed={value === preset.value}
        onClick={() => onSelect(preset.value)}
      >{preset.label}</button>)}
      <button
        ref={customButton}
        type="button"
        aria-haspopup="dialog"
        aria-controls={customOpen ? popoverId : undefined}
        aria-expanded={customOpen}
        aria-pressed={value === "custom"}
        onClick={() => onCustomOpenChange(!customOpen)}
      >{t("自定义")}</button>
    </div>
    <TooltipTrigger label={t("刷新")}><button className={controls.iconButton} aria-label={t("刷新")} disabled={busy} onClick={onRefresh}>
      <Icon className={busy ? controls.spin : ""} icon={refreshIcon} size="1.1em" />
    </button></TooltipTrigger>
    {customOpen && createPortal(<div
      id={popoverId}
      ref={popover}
      className={styles.popover}
      role="dialog"
      aria-label={t("自定义概览筛选")}
      style={position}
      onPointerDown={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          onCustomOpenChange(false);
          customButton.current?.focus();
        }
      }}
    >
      <div className={styles.quickPresets} aria-label={t("快捷时间范围")}>
        {quickPresets.map((preset) => <button
          key={preset.value}
          type="button"
          aria-pressed={quick === preset.value}
          onClick={() => onQuickSelect(preset.value === "four-hours" ? 4 * 60 * 60_000 : 24 * 60 * 60_000)}
        >{preset.label}</button>)}
      </div>
      <label><span>{t("开始时间")}</span><input type="text" placeholder={t("如：2026-08-23 09:00、1小时前")} value={customStart} onChange={(event) => onCustomStartChange(event.target.value)} /></label>
      <label><span>{t("结束时间")}</span><input type="text" placeholder={t("如：现在、2026-08-23 18:00")} value={customEnd} onChange={(event) => onCustomEndChange(event.target.value)} /></label>
      <div className={styles.filterRow}><ModelSelect mode="multiple" label={t("模型")} value={selectedModels} options={modelOptions} onChange={onSelectedModelsChange} /></div>
      <div className={styles.popoverActions}>
        <button type="button" className={controls.secondary} onClick={() => onCustomOpenChange(false)}>{t("取消")}</button>
        <button type="button" className={controls.primary} disabled={!customValid} onClick={onCustomApply}>{t("应用")}</button>
      </div>
    </div>, document.body)}
  </div>;
}
