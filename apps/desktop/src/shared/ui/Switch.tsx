import type { ButtonHTMLAttributes } from "react";
import styles from "./Switch.module.scss";

type SwitchProps = {
  checked: boolean;
  label: string;
  onChange: (checked: boolean) => void;
} & Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label" | "children" | "onChange" | "role">;

export function Switch({ checked, disabled, label, onChange, onClick, ...props }: SwitchProps) {
  return <button
    {...props}
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    disabled={disabled}
    className={[styles.root, props.className].filter(Boolean).join(" ")}
    data-checked={checked || undefined}
    onClick={(event) => {
      onClick?.(event);
      if (!event.defaultPrevented) onChange(!checked);
    }}
  ><span /></button>;
}
