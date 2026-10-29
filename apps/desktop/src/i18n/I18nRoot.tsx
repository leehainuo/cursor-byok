import { App } from "../App";
import { useI18n } from "./store";

export function I18nRoot() {
  useI18n();
  return <App />;
}
