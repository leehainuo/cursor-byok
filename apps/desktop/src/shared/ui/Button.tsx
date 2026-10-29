import type { ComponentProps } from "react";
import controls from "./Controls.module.scss";

export type ButtonVariant = "primary" | "secondary";
export type ButtonSize = "medium" | "small";

export function Button({
  variant = "secondary",
  size = "medium",
  className,
  type = "button",
  ...props
}: ComponentProps<"button"> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
}) {
  return (
    <button
      {...props}
      type={type}
      className={[controls[variant], size === "small" && controls.small, className].filter(Boolean).join(" ")}
    />
  );
}
