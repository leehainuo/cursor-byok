import type { ReactNode } from "react";
import { Card } from "./Card";
import styles from "./TitledCard.module.scss";

export function TitledCard({ title, action, children }: { title: ReactNode; action?: ReactNode; children: ReactNode }) {
  return <Card as="section" className={styles.root}><header className={styles.header}><div className={styles.title}>{title}</div>{action}</header>{children}</Card>;
}
