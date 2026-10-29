---
name: model-selection
description: Implement and review desktop selection of configured models through the unified grouped ModelSelect component. Use when adding or changing model dropdowns, model filters, Commit model settings, single-model selection, multi-model selection, provider grouping, or configured-model option construction under apps/desktop.
---

# Model Selection

Use one presentation component for every UI that selects from already configured models:

```text
apps/desktop/src/
├── shared/ui/
│   ├── ModelSelect.tsx             # Single/multiple selection behavior and grouped floating menu
│   └── ModelSelect.module.scss     # Trigger, provider headers, checkbox rows, and footer
├── shared/utils/
│   └── modelProvider.ts            # Built-in model provider display name
└── features/
    ├── home/                        # Builds overview multi-select options
    └── settings/                    # Builds Commit single-select options and persists selection

server/src/
├── cursor/services/commit_message.rs # Validates built-in/plugin IDs and creates Commit invocation
├── provider/router.rs                # Routes stable IDs to built-in or plugin providers
└── plugin/registry.rs                # Resolves plugin model IDs and executes plugin streams
```

## Component boundary

- Use `ModelSelect` when choosing one or more existing configured models.
- Do not implement another model dropdown, reuse generic `Select`, or add a compatibility wrapper for configured-model selection.
- Keep `ModelSelect` presentation-only. Pages own model/plugin state, option construction, filtering, and persistence.
- Keep editable model-ID entry in `Combobox`; creating or editing a model identifier is input, not configured-model selection.
- Delete replaced model-selection components, exports, styles, helpers, and compatibility paths once references are gone.

## Option contract

Construct every `ModelSelectOption` with:

- `value`: stable persisted/request identifier. Built-in models use `model_hash`; plugin models use plugin model `id`.
- `label`: user-facing model display name.
- `group`: supplier display name.
- `icon`/`iconSrc`: model/provider icon when available.

For built-in models, derive `group` with `modelProviderName(model)`. It uses the configured `group_name` first and the API hostname otherwise. For plugin models, use the localized provider display name.

Preserve source order within each supplier. The first occurrence of a supplier determines group order.

## Modes

```text
Single owner value: string
    └── <ModelSelect mode="single"> ── choose one ── close ── persist

Multiple owner value: string[]
    └── <ModelSelect mode="multiple"> ── toggle many ── remain open ── apply/filter
```

- Both modes render classic checkbox controls in option rows.
- Single mode allows exactly one checked option and closes immediately after selection.
- Multiple mode supports toggling, clearing, selecting all, and selecting none.
- In multiple mode, every supplier header has a checkbox: unchecked means none selected, checked means all selected, and indeterminate means some selected. Toggling it selects or clears that supplier.
- Indent child model rows relative to their supplier header so hierarchy remains visible.
- Commit always uses single mode and includes both configured built-in and configured plugin models. Its `直连` option has value `""`, belongs to the `Cursor` group, and is the first option.
- Commit settings follow the settings-card edit-state pattern: read mode shows the persisted model, Edit creates a local draft, selection only changes that draft, Cancel restores the persisted value, and Save persists once before returning to read mode.
- Persist the stable plugin model `id` unchanged. Commit generation validates that identifier through `PluginRegistry`, then lets `ProviderRouter` dispatch it; do not query the built-in model table for plugin IDs.
- Overview filtering uses multiple mode.

## Floating-menu invariants

Also apply the project `floating-ui` and `frontend` skills:

- Render the menu through a body portal and position it with `@floating-ui/dom`.
- Keep supplier headers and checkbox options inside the virtualized list; keep multi-select bulk actions outside it.
- Close on Escape and outside pointer interaction, then restore trigger focus.
- Preserve `aria-expanded`, `aria-controls`, `aria-haspopup`, listbox semantics, and multi-select semantics.
- Keep the trigger's open/focus border visible while the portaled menu owns focus.

## Review checklist

- Search the repository for old model-selection components and zero-reference model option helpers; delete them instead of retaining fallbacks.
- Confirm all configured-model selectors import `shared/ui/ModelSelect`.
- Confirm no feature implements checkbox selection, supplier grouping, portal positioning, or bulk actions independently.
- Confirm single/multiple value types cannot be mixed.
- Confirm Commit has `直连` first and cannot select multiple values.
- Confirm supplier labels are based on supplier identity, not request protocol type.
- Follow the user's validation instruction; when automated tests are not requested, report manual checks without running test or build commands.
