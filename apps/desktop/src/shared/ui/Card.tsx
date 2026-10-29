import type { ElementType, HTMLAttributes, ReactNode } from "react";
import styles from "./Card.module.scss";

type CardProps = HTMLAttributes<HTMLElement> & {
  as?: ElementType;
  children: ReactNode;
};

export function Card({ as: Component = "div", className, children, ...props }: CardProps) {
  return <Component {...props} className={[styles.root, className].filter(Boolean).join(" ")}>{children}</Component>;
}
