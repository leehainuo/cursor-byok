---
name: frontend
description: Implement and review the Cursor BYOK React/Tauri desktop frontend. Use for changes under apps/desktop involving routing, page layout, window chrome, settings UI, scrolling, virtualization, charts, themes, or frontend component architecture.
---

# Cursor BYOK Frontend

Build on the components and theme system already present in `apps/desktop`. Preserve the Tauri HTTP boundary; frontend management features call `/__byok-api__/api` and never add IPC business APIs.

## Desktop HTTP communication

- Treat the Rust management server as the single HTTP origin for the desktop UI. The main WebView and browser-opened detail pages must use `http://127.0.0.1:<dynamic-port>/__byok-api__/`; do not load the main UI from `tauri://localhost` or expose a second frontend origin.
- Make every frontend management request relative to `/__byok-api__/api`. Do not discover, inject, persist, or pass an `apiOrigin`, and do not construct management URLs from a fixed port.
- Keep the `/__byok-api__/` namespace reserved for this boundary:
  - `/__byok-api__/api/*` is handled locally by the Rust management API.
  - Other `/__byok-api__/*` paths are frontend documents, assets, modules, and development resources.
- In production, serve the frontend embedded by Tauri's `frontendDist` through the Rust server. Reuse Tauri's asset resolver rather than bundling or copying a second set of frontend resources.
- In development, let the Rust server reverse-proxy non-API `/__byok-api__/*` requests to Vite. Preserve the request path and query string. Do not configure Vite to proxy management API requests back to Rust.
- Configure Vite's base path as `/__byok-api__/` so generated assets, module imports, and development client URLs stay under the reserved namespace.
- Bind the development Vite server to an explicit loopback address compatible with the Rust proxy target; do not rely on `localhost` resolving to the same IP family.
- Open external detail pages with their normal loopback HTTP URL and hash route. Because the page and API are same-origin, do not add Tauri URL workarounds, API-origin query parameters, or CORS-dependent browser flows.
- Use Tauri commands only for native desktop capabilities such as opening Terminal, tray integration, or operating-system actions. Do not create IPC mirrors for HTTP business APIs.

## Component boundaries

- Keep Provider, Model, Call, Usage, and persisted settings state outside presentation components.
- Let components receive business snapshots and dispatch actions. Local React state is allowed for transient UI behavior such as focus, open state, drag state, and input drafts.
- Split pages, layouts, charts, forms, and reusable controls into focused components. Do not collect an entire feature set in `App.tsx`.
- Keep `App.tsx` as the route composition root.

## Page layout

- Build every full-height page with `components/layout/PageLayout.tsx`; do not hand-roll page shells with grid rows.
- Put page-level commands and controls such as create, import, export, filtering, sorting, and view options in the shared page action region through `PageActions`. Keep only operations that target one concrete list item, such as edit and delete, inside that item's row. Do not add duplicate action bars or panel-header controls inside page content.
- Never render a title region above a data table or flat data list. Use the page title for context, keep column labels in the table header, place page-level controls in `PageActions`, and wrap the table in a titleless `Card`. A hierarchy label that identifies parent data in a grouped parent-child view is not a table title.
- Keep the main scroll viewport full-height and `position: relative`; place the page title and action region absolutely over it.
- Reserve the absolute title/action region with `padding-top` on the scroll content, never by shortening or offsetting the scroll viewport itself.
- Keep the menu Card fixed and reserve it with the content area's `margin-left`; do not place Header, menu, and content in a flow grid.
- Do not use child or section padding to create external spacing. Use flex/grid gaps between siblings and margins or fixed offsets at container boundaries; the scroll-content top padding is reserved only for its absolute title/action overlay.
- Use a vertical flex layout with these invariants:
  - Header: `flex: 0 0 auto`.
  - Footer: `flex: 0 0 auto`.
  - Content: `flex: 1 1 auto`, `min-height: 0`, and `overflow: hidden`.
- Keep Header and Footer outside the scroll viewport. They must never grow or shrink with content.
- Apply macOS, Windows, and Linux window-corner safe-area variables only to edge chrome that can enter native rounded corners.
- Keep macOS traffic-light space and the top fixed drag strip free of interactive controls.
- Avoid decorative brand text and explanatory copy when it does not help the user perform an action or interpret state.

## Typography

- Write component styles in SCSS and source every `font-size` from `src/styles/_typography.scss`; never hardcode a font size in SCSS, React styles, or chart options.
- Use `base` for normal UI text and ordinary titles, including title bars, panels, cards, and section headings. Do not increase a font merely because its element is `h1` or `h2`.
- Use `xs` only for secondary information. Reserve `lg` for an explicitly oversized display title; do not use it for routine headings.
- Audit all typography-token usages after changing the scale, not only raw numeric font sizes.

## Scrolling and virtualization

- Use `components/layout/VirtualPage.tsx` for vertically scrolling page content.
- Use `components/virtual/VirtualList.tsx` for data collections and menus that can grow.
- Treat `ScrollArea` as the low-level primitive owned by the virtual scrolling implementation. Do not import `ScrollArea` directly in pages or layouts.
- Use the global `scroll-shadow-top` and `scroll-shadow-bottom` masks for scroll-edge shadows. When a scroll content inset reserves an absolute overlay, source the top shadow distance from the same CSS variable as its `padding-top`.
- Model non-list pages as a short sequence of measurable top-level virtual sections.
- Do not use document scrolling, native page `overflow: auto`, or nested vertical scroll containers.
- Ensure every flex/grid ancestor of a virtual viewport has `min-height: 0` and the viewport has an explicit bounded height.

## Routing and pages

- Use `HashRouter` for Tauri compatibility.
- Keep `/` as the statistics dashboard and expose every primary page through the persistent menu in the shared `AppLayout`.
- Use top-level routes for primary pages: `/`, `/providers`, `/models`, `/calls`, and `/settings`.
- Do not create separate Home and Settings shells or add a Home-switching control.
- Keep route content independent of the window Header and Footer.

## Charts

- Use ECharts through `components/charts/EChart.tsx`.
- Register only required ECharts Core charts, components, and renderers.
- Keep chart components declarative: accept domain data and construct an option without fetching or persisting data.
- Let `ResizeObserver` resize the chart with its layout container and dispose the instance on unmount.

## Validation

Run from `apps/desktop`:

```bash
npm run check
npm run tauri:build -- --debug --no-bundle
```

Verify that Header and Footer remain fixed, only the virtual content viewport scrolls, settings navigation remains reachable at minimum window size, and macOS controls stay outside rounded-corner and traffic-light unsafe regions.
