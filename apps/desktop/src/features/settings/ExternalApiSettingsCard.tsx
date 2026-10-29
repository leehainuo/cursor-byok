import { useEffect, useState } from "react";
import { api, type ExternalApiSettings } from "../../shared/api";
import { Button } from "../../shared/ui/Button";
import { FormField, SecretTextInput } from "../../shared/ui/FormControls";
import { Switch } from "../../shared/ui/Switch";
import { TitledCard } from "../../shared/ui/TitledCard";
import { useMessage } from "../../shared/ui/message";
import styles from "./ExternalApiSettingsCard.module.scss";

export function ExternalApiSettingsCard({ servicePort }: { servicePort: number }) {
  const message = useMessage();
  const [saved, setSaved] = useState<ExternalApiSettings | null>(null);
  const [draft, setDraft] = useState<ExternalApiSettings>({ enabled: false, api_key: "" });
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void api.externalApiSettings().then((settings) => {
      setSaved(settings);
      setDraft(settings);
    }).catch((cause: unknown) => message(cause instanceof Error ? cause.message : String(cause)));
  }, [message]);

  const save = async () => {
    try {
      setSaving(true);
      const settings = await api.setExternalApiSettings(draft);
      setSaved(settings);
      setDraft(settings);
      message(t("外部 API 设置已保存"));
    } catch (cause) {
      message(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSaving(false);
    }
  };

  const address = `http://127.0.0.1:${servicePort}/byok/v1`;
  const changed = saved && (saved.enabled !== draft.enabled || saved.api_key !== draft.api_key);

  return <TitledCard title={t("外部 API")} action={<Button size="small" variant="primary" disabled={!changed || saving} onClick={() => void save()}>
    {saving ? t("保存中…") : t("保存")}
  </Button>}>
    <div className={styles.content}>
      <div className={styles.row}>
        <div className={styles.description}>
          <strong>{t("开启外部 API")}</strong>
          <small>{t("允许本机应用使用密钥调用已配置的模型，包括插件模型。")}</small>
        </div>
        <Switch label={t("开启外部 API")} checked={draft.enabled} disabled={!saved || saving}
          onChange={(enabled) => setDraft((current) => ({ ...current, enabled }))} />
      </div>
      <FormField label={t("API 密钥")} hint={t("开启后，所有外部请求都必须提供此密钥。")}> 
        <SecretTextInput value={draft.api_key} autoComplete="off" disabled={!saved || saving}
          onChange={(event) => setDraft((current) => ({ ...current, api_key: event.target.value }))} />
      </FormField>
      <div className={styles.address}>
        <strong>{t("基础地址")}</strong>
        <code>{address}</code>
      </div>
    </div>
  </TitledCard>;
}
