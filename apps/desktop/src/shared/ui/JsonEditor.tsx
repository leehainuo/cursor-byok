import { useEffect, useRef } from "react";
import type * as Monaco from "monaco-editor/editor/editor.api";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/language/json/json.worker?worker";
import styles from "./JsonEditor.module.scss";

type MonacoApi = typeof Monaco;

let monacoPromise: Promise<MonacoApi> | null = null;

function loadMonaco() {
  if (!monacoPromise) {
    self.MonacoEnvironment = {
      getWorker: (_moduleId, label) => label === "json" ? new JsonWorker() : new EditorWorker(),
    };
    monacoPromise = Promise.all([
      import("monaco-editor/editor/editor.api"),
      import("monaco-editor/language/json/monaco.contribution"),
    ]).then(([monaco]) => monaco);
  }
  return monacoPromise;
}

function json(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return null;
  }
}

export function formatStructuredText(value: string) {
  const whole = json(value);
  if (whole !== null) return whole;

  let changed = false;
  const formatted = value.split("\n").map((line) => {
    const match = /^(\s*data:\s*)(.*)$/.exec(line);
    if (!match || match[2] === "[DONE]") return line;
    const payload = json(match[2]);
    if (payload === null) return line;
    changed = true;
    return `${match[1]}${payload}`;
  }).join("\n");
  return changed ? formatted : value;
}

function hexByte(value: number) {
  return Math.round(Math.min(255, Math.max(0, value))).toString(16).padStart(2, "0");
}

function monacoColor(value: string, fallback: string) {
  const color = value.trim() || fallback;
  const hex = /^#([\da-f]{3,4}|[\da-f]{6}|[\da-f]{8})$/i.exec(color);
  if (hex) {
    const digits = hex[1];
    return digits.length <= 4
      ? `#${[...digits].map((digit) => digit.repeat(2)).join("")}`
      : `#${digits}`;
  }

  const rgb = /^rgba?\((.*)\)$/i.exec(color);
  if (rgb) {
    const parts = rgb[1].replace(/\s*[,/]\s*/g, " ").trim().split(/\s+/);
    if (parts.length === 3 || parts.length === 4) {
      const channels = parts.slice(0, 3).map((part) => {
        const number = Number.parseFloat(part);
        return part.endsWith("%") ? number * 2.55 : number;
      });
      const alphaPart = parts[3];
      const alpha = alphaPart == null
        ? 255
        : (alphaPart.endsWith("%") ? Number.parseFloat(alphaPart) / 100 : Number.parseFloat(alphaPart)) * 255;
      if ([...channels, alpha].every(Number.isFinite)) {
        return `#${channels.map(hexByte).join("")}${alpha === 255 ? "" : hexByte(alpha)}`;
      }
    }
  }

  return fallback;
}

function themeColor(styles: CSSStyleDeclaration, name: string, fallback: string) {
  return monacoColor(styles.getPropertyValue(name), fallback);
}

function applyEditorTheme(monaco: MonacoApi) {
  const rootStyles = getComputedStyle(document.documentElement);
  const light = document.documentElement.dataset.theme === "default-light";
  monaco.editor.defineTheme("cursor-byok", {
    base: light ? "vs" : "vs-dark",
    inherit: true,
    rules: [],
    colors: {
      "editor.background": themeColor(rootStyles, "--vscode-input-background", light ? "#ffffff" : "#222222"),
      "editor.foreground": themeColor(rootStyles, "--vscode-input-foreground", light ? "#202020" : "#ffffffe6"),
      "editorCursor.foreground": themeColor(rootStyles, "--vscode-editorCursor-foreground", light ? "#202020" : "#ffffff"),
      "editorLineNumber.foreground": themeColor(rootStyles, "--vscode-editorLineNumber-foreground", light ? "#999999" : "#ffffff40"),
      "editorLineNumber.activeForeground": themeColor(rootStyles, "--vscode-editorLineNumber-activeForeground", light ? "#202020" : "#ffffffe6"),
      "editor.selectionBackground": themeColor(rootStyles, "--vscode-editor-selectionBackground", light ? "#0069cc33" : "#49b0ff33"),
    },
  });
  monaco.editor.setTheme("cursor-byok");
}

export function JsonEditor({ value, onChange, readOnly = false, autoFormat = true, detail = false, ariaLabel }: {
  value: string;
  onChange?: (value: string) => void;
  readOnly?: boolean;
  autoFormat?: boolean;
  detail?: boolean;
  ariaLabel: string;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const hostRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null);
  const modelRef = useRef<Monaco.editor.ITextModel | null>(null);
  const onChangeRef = useRef(onChange);
  const valueRef = useRef(value);
  const readOnlyRef = useRef(readOnly);
  const autoFormatRef = useRef(autoFormat);
  const ariaLabelRef = useRef(ariaLabel);
  const changingRef = useRef(false);
  onChangeRef.current = onChange;
  valueRef.current = value;
  readOnlyRef.current = readOnly;
  autoFormatRef.current = autoFormat;
  ariaLabelRef.current = ariaLabel;

  const format = () => {
    const model = modelRef.current;
    if (!model) return;
    const formatted = formatStructuredText(model.getValue());
    if (formatted === model.getValue()) return;
    changingRef.current = true;
    model.setValue(formatted);
    changingRef.current = false;
    onChangeRef.current?.(formatted);
  };

  useEffect(() => {
    const host = hostRef.current;
    const root = rootRef.current;
    if (!host || !root) return;
    let disposed = false;
    let contentSubscription: Monaco.IDisposable | null = null;
    let blurSubscription: Monaco.IDisposable | null = null;
    let themeObserver: MutationObserver | null = null;
    void loadMonaco().then((monaco) => {
      if (disposed) return;
      applyEditorTheme(monaco);
      const rootStyles = getComputedStyle(root);
      const fontSize = Number.parseFloat(rootStyles.getPropertyValue("--json-editor-font-size"));
      const fontFamily = rootStyles.getPropertyValue("--oa-code-font").trim();
      const initialValue = autoFormatRef.current ? formatStructuredText(valueRef.current) : valueRef.current;
      const model = monaco.editor.createModel(initialValue, "json");
      const editor = monaco.editor.create(host, {
        model,
        theme: "cursor-byok",
        ariaLabel: ariaLabelRef.current,
        readOnly: readOnlyRef.current,
        domReadOnly: readOnlyRef.current,
        automaticLayout: true,
        fontFamily: fontFamily || undefined,
        fontSize: Number.isFinite(fontSize) ? fontSize : undefined,
        minimap: { enabled: false },
        glyphMargin: false,
        folding: true,
        lineNumbersMinChars: 3,
        overviewRulerLanes: 0,
        overviewRulerBorder: false,
        renderLineHighlight: "none",
        scrollBeyondLastLine: false,
        smoothScrolling: true,
        wordWrap: "off",
        padding: { top: 8, bottom: 16 },
        stickyScroll: { enabled: false },
        contextmenu: true,
        formatOnPaste: !readOnlyRef.current,
        formatOnType: !readOnlyRef.current,
        scrollbar: { alwaysConsumeMouseWheel: false },
      });
      editorRef.current = editor;
      modelRef.current = model;
      const remeasure = () => {
        if (disposed) return;
        monaco.editor.remeasureFonts();
        editor.layout();
      };
      requestAnimationFrame(remeasure);
      void document.fonts?.ready.then(remeasure);
      contentSubscription = model.onDidChangeContent(() => {
        if (!changingRef.current) onChangeRef.current?.(model.getValue());
      });
      blurSubscription = editor.onDidBlurEditorWidget(() => {
        if (!readOnlyRef.current && autoFormatRef.current) format();
      });
      themeObserver = new MutationObserver(() => applyEditorTheme(monaco));
      themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    });
    return () => {
      disposed = true;
      themeObserver?.disconnect();
      blurSubscription?.dispose();
      contentSubscription?.dispose();
      editorRef.current?.dispose();
      modelRef.current?.dispose();
      editorRef.current = null;
      modelRef.current = null;
    };
  }, []);

  useEffect(() => {
    const model = modelRef.current;
    const next = readOnly && autoFormat ? formatStructuredText(value) : value;
    if (!model || model.getValue() === next) return;
    changingRef.current = true;
    model.setValue(next);
    changingRef.current = false;
  }, [autoFormat, readOnly, value]);

  useEffect(() => {
    editorRef.current?.updateOptions({ readOnly, domReadOnly: readOnly, ariaLabel });
  }, [ariaLabel, readOnly]);

  return <div ref={rootRef} className={[styles.root, detail && styles.detail].filter(Boolean).join(" ")}>
    <div className={styles.toolbar}>
      <button type="button" className={styles.formatButton} onClick={format}>{t("格式化")}</button>
    </div>
    <div ref={hostRef} className={styles.host} />
  </div>;
}
