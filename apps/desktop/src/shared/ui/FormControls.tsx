import { useState, type InputHTMLAttributes } from "react";
import { Icon } from "./Icon";
import { TooltipTrigger } from "./TooltipTrigger";
import { eyeIcon, eyeOffIcon, informationOutlineIcon } from "./icons";
import styles from "./FormControls.module.scss";

export function TextInput(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={[styles.input, props.className].filter(Boolean).join(" ")} />;
}

export function SecretTextInput({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  const [visible, setVisible] = useState(false);
  return <div className={styles.secret}>
    <input {...props} type={visible ? "text" : "password"} className={[styles.input, className].filter(Boolean).join(" ")} />
    <button type="button" className={styles.secretToggle} aria-label={visible ? t("隐藏敏感内容") : t("显示敏感内容")} onClick={() => setVisible((current) => !current)}>
      <Icon icon={visible ? eyeOffIcon : eyeIcon} size="1.1em" />
    </button>
  </div>;
}

export function FormField({ label, hint, className, children }: { label: string; hint?: string; className?: string; children: React.ReactNode }) {
  return <label className={[styles.field, className].filter(Boolean).join(" ")}>
    <div className={styles.label}>
      <div>{label}</div>
      {hint && <TooltipTrigger label={hint}><div className={styles.hint}><Icon icon={informationOutlineIcon} size="1.1em" /></div></TooltipTrigger>}
    </div>
    {children}
  </label>;
}
