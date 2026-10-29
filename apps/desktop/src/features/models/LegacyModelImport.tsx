import { useState, type ReactNode } from "react";
import { api, type LegacyModelImportPreview } from "../../shared/api";
import { appStore } from "../../shared/store/appStore";
import { ConfirmDialog } from "../../shared/ui/ConfirmDialog";
import { useMessage } from "../../shared/ui/message";
import styles from "./LegacyModelImport.module.scss";

type LegacyModelImportControl = {
  busy: boolean;
  previewing: boolean;
  open: () => void;
};

export function LegacyModelImport({ children }: { children: (control: LegacyModelImportControl) => ReactNode }) {
  const message = useMessage();
  const [preview, setPreview] = useState<LegacyModelImportPreview | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [importing, setImporting] = useState(false);

  const open = async () => {
    try {
      setPreviewing(true);
      setPreview(await api.previewV0049Models());
    } catch (cause) {
      message(errorText(cause));
    } finally {
      setPreviewing(false);
    }
  };

  const confirm = async () => {
    try {
      setImporting(true);
      const result = await appStore.importV0049Models();
      if (!result) {
        const error = appStore.getSnapshot().error;
        if (error) message(error);
        return;
      }
      setPreview(null);
      message(result.imported > 0
        ? t("导入完成：新增 {imported} 个模型，跳过 {skipped} 个已存在模型", { imported: result.imported, skipped: result.skipped })
        : t("配置中的 {count} 个模型均已存在，无需重复导入", { count: result.skipped }));
    } catch (cause) {
      message(errorText(cause));
    } finally {
      setImporting(false);
    }
  };

  return <>
    {children({ busy: previewing || importing, previewing, open: () => void open() })}
    <ConfirmDialog
      open={preview !== null}
      title={t("确认导入旧版模型配置")}
      busy={importing}
      wide
      cancelLabel={t("取消")}
      confirmLabel={t("确认导入")}
      onCancel={() => setPreview(null)}
      onConfirm={() => void confirm()}
    >
      {preview && <div className={styles.content}>
        <div className={styles.source}>
          <strong>{t("配置文件")}</strong>
          <code>{preview.source}</code>
        </div>
        <div className={styles.counts}>
          <div><strong>{preview.total}</strong><small>{t("配置模型")}</small></div>
          <div><strong>{preview.new_models}</strong><small>{t("将新增")}</small></div>
          <div><strong>{preview.existing_models}</strong><small>{t("已存在")}</small></div>
        </div>
        <div className={styles.models}>
          {preview.models.map((model) => <div className={styles.model} key={model.model_hash}>
            <div>
              <strong>{model.display_name}</strong>
              <small>{model.model_id} · {model.type === "openai" ? "OpenAI" : "Anthropic"}</small>
            </div>
            <span className={model.existing ? styles.existing : styles.new}>
              {model.existing ? t("已存在，跳过") : t("新增")}
            </span>
          </div>)}
        </div>
        <small className={styles.hint}>{t("重复导入相同配置不会创建重复模型；已经存在的模型会自动跳过。")}</small>
      </div>}
    </ConfirmDialog>
  </>;
}

function errorText(cause: unknown) {
  return cause instanceof Error ? cause.message : String(cause);
}
