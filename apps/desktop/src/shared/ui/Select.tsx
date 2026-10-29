import { autoUpdate, computePosition, flip, offset, shift, size } from "@floating-ui/dom";
import { forwardRef, useEffect, useId, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { VirtualList } from "../virtual/VirtualList";
import type { VirtualListApi } from "../virtual/virtualTypes";
import { Icon, type IconProps } from "./Icon";
import { chevronDownIcon } from "./icons";
import styles from "./Select.module.scss";

export type SelectOption = { value: string; label: string; icon?: IconProps["icon"]; iconSrc?: string };

export function Select({ value, options, disabled, ariaLabel, onChange }: { value: string; options: SelectOption[]; disabled?: boolean; ariaLabel: string; onChange: (value: string) => void }) {
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const listApi = useRef<VirtualListApi | null>(null);
  const menuId = useId();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [position, setPosition] = useState({ left: 0, top: 0, width: 0, maxHeight: 280 });
  const selected = options.find((option) => option.value === value);

  useLayoutEffect(() => {
    if (!open || !button.current || !menu.current) return;
    return autoUpdate(button.current, menu.current, () => void computePosition(button.current!, menu.current!, {
      placement: "bottom-start",
      middleware: [offset(5), flip({ padding: 10 }), shift({ padding: 10 }), size({ padding: 10, apply: ({ rects, availableHeight }) => setPosition((current) => ({ ...current, width: rects.reference.width, maxHeight: Math.max(120, availableHeight) })) })],
    }).then(({ x, y }) => setPosition((current) => ({ ...current, left: x, top: y }))));
  }, [open]);
  useEffect(() => {
    if (open && active >= 0) listApi.current?.scrollToIndex(active);
  }, [active, open]);
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => { if (!button.current?.contains(event.target as Node) && !menu.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);

  const move = (step: number) => {
    if (!options.length) return;
    setOpen(true);
    setActive((index) => (index + step + options.length) % options.length);
  };
  const choose = (option: SelectOption) => { onChange(option.value); setOpen(false); button.current?.focus(); };
  return <>
    <button ref={button} type="button" className={styles.trigger} aria-label={ariaLabel} aria-haspopup="listbox" aria-controls={open ? menuId : undefined} aria-expanded={open} disabled={disabled} onClick={() => { const nextOpen = !open; setOpen(nextOpen); if (nextOpen) setActive(Math.max(0, options.findIndex((option) => option.value === value))); }} onKeyDown={(event) => {
      if (event.key === "ArrowDown") { event.preventDefault(); move(1); }
      if (event.key === "ArrowUp") { event.preventDefault(); move(-1); }
      if (event.key === "Enter" && open) { event.preventDefault(); choose(options[active]); }
      if (event.key === "Escape") setOpen(false);
    }}><span className={styles.optionContent}>{(selected?.icon || selected?.iconSrc) && <Icon icon={selected.icon} src={selected.iconSrc} />}<span>{selected?.label ?? value}</span></span><Icon icon={chevronDownIcon} size="1.1em" className={[styles.dropdownIcon, open && styles.dropdownIconOpen].filter(Boolean).join(" ")} /></button>
    {open && createPortal(<div id={menuId} ref={menu} className={styles.menu} role="listbox" style={{ left: position.left, top: position.top, width: position.width }}>
      <VirtualList items={options} itemKey="value" estimatedItemHeight={30} onReady={(api) => { listApi.current = api; api.scrollToIndex(active); }} style={{ height: Math.min(options.length * 30, Math.max(30, position.maxHeight - 8)) }}>
        {(option, index) => <button type="button" role="option" aria-selected={option.value === value} data-active={index === active || undefined} onMouseEnter={() => setActive(index)} onClick={() => choose(option)}><span className={styles.optionContent}>{(option.icon || option.iconSrc) && <Icon icon={option.icon} src={option.iconSrc} />}<span>{option.label}</span></span></button>}
      </VirtualList>
    </div>, document.body)}
  </>;
}

export type ComboboxHandle = {
  openAll: () => void;
};

type ComboboxProps = {
  value: string;
  options?: string[];
  placeholder?: string;
  disabled?: boolean;
  append?: ReactNode;
  onChange: (value: string) => void;
};

export const Combobox = forwardRef<ComboboxHandle, ComboboxProps>(function Combobox({ value, options = [], placeholder, disabled, append, onChange }, ref) {
  const root = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const listApi = useRef<VirtualListApi | null>(null);
  const closeTimer = useRef<number | null>(null);
  const menuId = useId();
  const [open, setOpen] = useState(false);
  const [filtering, setFiltering] = useState(true);
  const [active, setActive] = useState(-1);
  const [position, setPosition] = useState({ left: 0, top: 0, width: 0, maxHeight: 280 });
  const filtered = useMemo(() => {
    if (!filtering) return options;
    const query = value.trim().toLocaleLowerCase();
    return options.filter((option) => !query || option.toLocaleLowerCase().includes(query));
  }, [filtering, options, value]);

  useImperativeHandle(ref, () => ({
    openAll: () => {
      if (closeTimer.current !== null) window.clearTimeout(closeTimer.current);
      setFiltering(false);
      setActive(0);
      setOpen(true);
    },
  }), []);

  useLayoutEffect(() => {
    if (!open || !root.current || !menu.current) return;
    return autoUpdate(root.current, menu.current, () => void computePosition(root.current!, menu.current!, {
      placement: "bottom-start",
      middleware: [offset(6), flip({ padding: 12 }), shift({ padding: 12 }), size({ padding: 12, apply: ({ rects, availableHeight }) => setPosition((current) => ({ ...current, width: rects.reference.width, maxHeight: Math.max(160, availableHeight) })) })],
    }).then(({ x, y }) => setPosition((current) => ({ ...current, left: x, top: y }))));
  }, [open, filtered.length]);

  useEffect(() => () => {
    if (closeTimer.current !== null) window.clearTimeout(closeTimer.current);
  }, []);
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node) && !menu.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);
  useEffect(() => {
    if (open && active >= 0) listApi.current?.scrollToIndex(active);
  }, [active, open]);

  const openMenu = () => {
    if (disabled || options.length === 0) return;
    setFiltering(true);
    setOpen(true);
    const query = value.trim().toLocaleLowerCase();
    const matchingOptions = options.filter((option) => !query || option.toLocaleLowerCase().includes(query));
    const selected = matchingOptions.indexOf(value);
    setActive(selected >= 0 ? selected : 0);
  };
  const choose = (option: string) => { onChange(option); setOpen(false); input.current?.focus(); };
  const move = (step: number) => {
    if (!open) { openMenu(); return; }
    if (!filtered.length) return;
    setActive((index) => (index < 0 ? 0 : (index + step + filtered.length) % filtered.length));
  };
  return <div className={styles.comboRow}><div ref={root} className={styles.combo}>
    <input ref={input} value={value} placeholder={placeholder} disabled={disabled} role="combobox" aria-haspopup="listbox" aria-controls={open ? menuId : undefined} aria-expanded={open} aria-autocomplete="list" onFocus={() => { if (options.length) openMenu(); }} onBlur={() => { closeTimer.current = window.setTimeout(() => setOpen(false), 100); }} onChange={(event) => { setFiltering(true); onChange(event.target.value); setActive(0); if (options.length) setOpen(true); }} onKeyDown={(event) => {
      if (event.key === "ArrowDown") { event.preventDefault(); move(1); }
      if (event.key === "ArrowUp") { event.preventDefault(); move(-1); }
      if (event.key === "Enter" && open && filtered[active]) { event.preventDefault(); choose(filtered[active]); }
      if (event.key === "Escape") setOpen(false);
    }} />
    <button type="button" className={styles.comboToggle} disabled={disabled} aria-label={t("打开模型列表")} aria-expanded={open} tabIndex={-1} onMouseDown={(event) => event.preventDefault()} onClick={() => { if (open) setOpen(false); else { openMenu(); input.current?.focus(); } }}><Icon icon={chevronDownIcon} size="1.1em" className={[styles.dropdownIcon, open && styles.dropdownIconOpen].filter(Boolean).join(" ")} /></button>
    {open && createPortal(<div id={menuId} ref={menu} className={styles.menu} role="listbox" style={{ left: position.left, top: position.top, width: position.width }}>
      <VirtualList items={filtered} itemKey={(option) => option} estimatedItemHeight={30} onReady={(api) => { listApi.current = api; if (active >= 0) api.scrollToIndex(active); }} style={{ height: Math.min(filtered.length * 30, Math.max(30, position.maxHeight - 8)) }}>
        {(option, index) => <button type="button" role="option" aria-selected={option === value} data-active={index === active || undefined} onMouseEnter={() => setActive(index)} onMouseDown={(event) => event.preventDefault()} onClick={() => choose(option)}>{option}</button>}
      </VirtualList>
    </div>, document.body)}
  </div>{append}</div>;
});

