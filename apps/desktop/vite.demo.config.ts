import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { staticI18nPlugin } from "./plugins/static-i18n-plugin.ts";

const desktopRoot = fileURLToPath(new URL("./", import.meta.url));
const demoInput = fileURLToPath(new URL("./demo/index.html", import.meta.url));
const demoOutput = fileURLToPath(new URL("../docs/public/product-demo", import.meta.url));

export default defineConfig({
  root: desktopRoot,
  base: "/product-demo/",
  plugins: [staticI18nPlugin(), react()],
  publicDir: false,
  build: {
    outDir: demoOutput,
    emptyOutDir: true,
    rollupOptions: {
      input: demoInput,
    },
  },
});
