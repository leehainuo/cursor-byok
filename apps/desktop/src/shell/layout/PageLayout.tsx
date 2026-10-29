import type { ElementType, ReactNode } from "react";
import styles from "./PageLayout.module.scss";

type PageLayoutProps = {
  as?: ElementType;
  className?: string;
  header?: ReactNode;
  footer?: ReactNode;
  children: ReactNode;
};

export function PageLayout({ as: Component = "div", className, header, footer, children }: PageLayoutProps) {
  return <Component className={[styles.root, className].filter(Boolean).join(" ")}>
    {header && <div className={styles.header}>{header}</div>}
    <div className={styles.body}>{children}</div>
    {footer && <div className={styles.footer}>{footer}</div>}
  </Component>;
}
