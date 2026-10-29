import ReactDOM from "react-dom/client";
import "../node_modules/monaco-editor/min/vs/editor/editor.main.css";
import { I18nRoot } from "./i18n/I18nRoot";
import { initializeI18n } from "./i18n/store";
import { appStore } from "./shared/store/appStore";
import { applyTheme } from "./shared/theme/theme";
import "./styles/globals.scss";

initializeI18n();
applyTheme(appStore.getSnapshot().theme);
void appStore.refresh();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <I18nRoot />,
);
