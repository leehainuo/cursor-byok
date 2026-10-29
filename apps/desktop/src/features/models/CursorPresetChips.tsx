import type { ModelType } from "../../shared/api";
import { modelPresets, presetEndpoint, trimTrailingSlash, type ModelPreset } from "../../shared/utils/modelPresets";
import styles from "./CursorPresetChips.module.scss";

/** 常用服务商预设：点击按当前协议类型自动填充对应端点与默认模型 */
export function CursorPresetChips({ type, baseUrl, onPick }: { type: ModelType; baseUrl: string; onPick: (preset: ModelPreset) => void }) {
  return <div className={styles.wrap}>
    <span className={styles.label}>{t("常用预设")}</span>
    <div className={styles.chips}>
      {modelPresets.map((preset) => {
        const active = trimTrailingSlash(baseUrl) === trimTrailingSlash(presetEndpoint(preset, type).baseUrl);
        return <button
          type="button"
          key={preset.key}
          className={active ? `${styles.chip} ${styles.active}` : styles.chip}
          title={preset.keyHint}
          onClick={() => onPick(preset)}
        >
          <img className={styles.icon} src={preset.icon} alt="" />
          {preset.name}
        </button>;
      })}
    </div>
  </div>;
}
