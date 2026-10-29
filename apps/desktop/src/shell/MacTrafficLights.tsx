import styles from "./MacTrafficLights.module.scss";

export function MacTrafficLights() {
  return <div className={styles.root} aria-hidden="true">
    <span className={[styles.light, styles.close].join(" ")} />
    <span className={[styles.light, styles.minimize].join(" ")} />
    <span className={[styles.light, styles.zoom].join(" ")} />
  </div>;
}
