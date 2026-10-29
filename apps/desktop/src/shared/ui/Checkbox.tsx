import { Icon } from "./Icon";
import { checkIcon } from "./icons";
import styles from "./Checkbox.module.scss";

export function Checkbox({ checked, disabled, label, onChange }: {
  checked: boolean;
  disabled?: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return <label className={styles.root}>
    <input type="checkbox" checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} />
    <span className={styles.control} aria-hidden="true">{checked && <Icon icon={checkIcon} size="0.875em" />}</span>
    <span>{label}</span>
  </label>;
}
