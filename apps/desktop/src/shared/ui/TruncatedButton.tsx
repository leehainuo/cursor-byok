import { useRef, useState, type ComponentProps } from "react";
import { Button } from "./Button";
import { TooltipTrigger } from "./TooltipTrigger";
import styles from "./TruncatedButton.module.scss";

/**
 * 文本被省略号截断时才显示完整文案悬浮提示的按钮。
 * 按钮是 flex 容器,省略号只作用在内层文本 span 上;
 * 截断在悬停/聚焦时现测——挂载时字体可能未加载,提前测会得到错误结果。
 */
export function TruncatedButton({ label, ...props }: ComponentProps<typeof Button> & { label: string }) {
  const element = useRef<HTMLSpanElement>(null);
  const [truncated, setTruncated] = useState(false);
  const measure = () => {
    const text = element.current;
    if (text) setTruncated(text.scrollWidth > text.clientWidth);
  };
  const button = <Button {...props} onPointerEnter={measure} onFocus={measure}>
    <span ref={element} className={styles.label}>{label}</span>
  </Button>;
  return truncated ? <TooltipTrigger label={label}>{button}</TooltipTrigger> : button;
}
