import { Icon as IconifyIcon, type IconifyIcon as IconData } from "@iconify/react/offline";
import styles from "./Icon.module.scss";

export interface IconProps {
  icon?: IconData;
  src?: string;
  size?: `${number}em`;
  className?: string;
}

export function Icon({ icon, src, size = "1em", className }: IconProps) {
  return <div
    aria-hidden="true"
    className={[styles.icon, className].filter(Boolean).join(" ")}
    style={{ height: size, width: size }}
  >
    {src ? <img alt="" src={src} /> : icon ? <IconifyIcon height="100%" icon={icon} width="100%" /> : null}
  </div>;
}
