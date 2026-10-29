# Semble 与 CodeGraph React/Vue 对比基准

- 环境：macos aarch64，rustc 1.97.1 (8bab26f4f 2026-07-14)
- 查询：Top 10，每条重复 5 次
- natural_language：英文行为描述；literal：多词代码/错误字面片段；symbol：精确符号名
- 对比：natural_language、literal 使用 CodeGraph explore；symbol 使用 CodeGraph searchNodes
- 判定：返回代码范围必须覆盖人工标注的实现行

## 整体效果

| 系统 | 查询轨道 | Recall@1 | Recall@5 | Recall@10 | MRR@10 | nDCG@10 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| CodeGraph | natural_language | 10.0% | 20.0% | 20.0% | 0.150 | 0.163 |
| CodeGraph | literal | 25.0% | 25.0% | 25.0% | 0.250 | 0.250 |
| CodeGraph | symbol | 50.0% | 50.0% | 50.0% | 0.500 | 0.500 |
| Semble | natural_language | 55.0% | 95.0% | 95.0% | 0.729 | 0.786 |
| Semble | literal | 60.0% | 100.0% | 100.0% | 0.750 | 0.813 |
| Semble | symbol | 100.0% | 100.0% | 100.0% | 1.000 | 1.000 |

## 性能对比

冷启动就绪包含运行时加载和首次索引；Semble 查询在一秒刷新窗口内直接复用已检查索引，窗口到期或显式 refresh 时重新校验源码指纹。缓存查询与 refresh＋symbol 分开计时；查询耗时均为持久进程中的实际工具处理耗时，不含 CLI 进程启动。每条查询先预热一次。

| 数据集 | 系统 | 冷启动就绪 | 缓存加载 | 自然语言 P50 / P95 / σ | 字面 P50 / P95 / σ | 缓存符号 P50 / P95 / σ | 刷新＋符号 P50 / P95 | 文件 / 索引单元 | 索引体积 |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| react | Semble 0.1.0 | 5797.8 ms | 65.3 ms | 11.22 / 12.39 / 0.57 ms | 11.21 / 12.64 / 2.50 ms | 0.01 / 0.01 / 0.00 ms | 13.89 / 15.40 ms | 4760 / 45275 chunks | 57.35 MiB |
| react | CodeGraph 1.0.1 | 36601.0 ms | 96.0 ms | 230.27 / 275.19 / 32.93 ms | 235.11 / 294.48 / 30.22 ms | 2.73 / 14.48 / 4.87 ms | — | 4584 / 53295 nodes / 163188 edges | 317.67 MiB |
| vue | Semble 0.1.0 | 826.1 ms | 12.1 ms | 1.88 / 2.18 / 0.18 ms | 1.96 / 2.41 / 0.23 ms | 0.01 / 0.01 / 0.00 ms | 4.66 / 4.93 ms | 554 / 7933 chunks | 9.24 MiB |
| vue | CodeGraph 1.0.1 | 5232.5 ms | 46.8 ms | 55.94 / 72.82 / 8.29 ms | 41.13 / 50.20 / 3.69 ms | 0.57 / 1.96 / 0.66 ms | — | 552 / 6381 nodes / 44277 edges | 33.33 MiB |

## 分数据集效果

| 数据集 | 系统 | 轨道 | Recall@1 / @5 / @10 | MRR@10 | nDCG@10 |
| --- | --- | --- | ---: | ---: | ---: |
| react | Semble | natural_language | 70.0% / 100.0% / 100.0% | 0.850 | 0.889 |
| react | Semble | literal | 70.0% / 100.0% / 100.0% | 0.792 | 0.843 |
| react | Semble | symbol | 100.0% / 100.0% / 100.0% | 1.000 | 1.000 |
| react | CodeGraph | natural_language | 0.0% / 0.0% / 0.0% | 0.000 | 0.000 |
| react | CodeGraph | literal | 10.0% / 10.0% / 10.0% | 0.100 | 0.100 |
| react | CodeGraph | symbol | 20.0% / 20.0% / 20.0% | 0.200 | 0.200 |
| vue | Semble | natural_language | 40.0% / 90.0% / 90.0% | 0.608 | 0.682 |
| vue | Semble | literal | 50.0% / 100.0% / 100.0% | 0.708 | 0.782 |
| vue | Semble | symbol | 100.0% / 100.0% / 100.0% | 1.000 | 1.000 |
| vue | CodeGraph | natural_language | 20.0% / 40.0% / 40.0% | 0.300 | 0.326 |
| vue | CodeGraph | literal | 40.0% / 40.0% / 40.0% | 0.400 | 0.400 |
| vue | CodeGraph | symbol | 80.0% / 80.0% / 80.0% | 0.800 | 0.800 |

## react · Semble · natural_language 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `state-hook-api` | 1 | 11.16 / 11.40 / 0.10 ms | `packages/react/src/ReactHooks.js:44-66` |
| `create-context` | 1 | 12.25 / 12.44 / 0.16 ms | `packages/react/src/ReactContext.js:1-18` |
| `schedule-fiber-update` | 1 | 10.89 / 11.11 / 0.12 ms | `packages/react-reconciler/src/ReactFiberWorkLoop.js:972-1005` |
| `commit-root` | 2 | 12.02 / 12.39 / 0.21 ms | `packages/react-reconciler/src/ReactFiberDevToolsHook.js:84-111` |
| `suspense-begin-work` | 1 | 10.70 / 10.89 / 0.13 ms | `packages/react-reconciler/src/ReactFiberBeginWork.js:2369-2387` |
| `delegated-dom-events` | 1 | 10.28 / 10.55 / 0.18 ms | `packages/react-dom-bindings/src/events/DOMPluginEventSystem.js:430-433` |
| `create-dom-root` | 2 | 11.56 / 11.69 / 0.07 ms | `packages/react-dom/src/client/ReactDOMRoot.js:99-117` |
| `reconcile-keyed-array` | 1 | 11.22 / 11.41 / 0.18 ms | `packages/react-reconciler/src/ReactChildFiber.js:1160-1182` |
| `dispatch-state-update` | 2 | 11.09 / 11.23 / 0.08 ms | `packages/react-reconciler/src/ReactFiberHooks.js:3654-3679` |
| `hydrate-dom-root` | 1 | 11.45 / 11.61 / 0.07 ms | `packages/react-dom/src/client/ReactDOMRootFB.js:134-169` |

## react · Semble · literal 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `state-hook-api` | 4 | 11.45 / 11.72 / 0.17 ms | `packages/react-server/src/ReactFizzHooks.js:406-428` |
| `create-context` | 1 | 12.60 / 12.64 / 0.10 ms | `packages/react/src/ReactContext.js:1-18` |
| `schedule-fiber-update` | 1 | 11.30 / 11.48 / 0.12 ms | `packages/react-reconciler/src/ReactFiberWorkLoop.js:972-1005` |
| `commit-root` | 1 | 11.45 / 11.55 / 0.13 ms | `packages/react-reconciler/src/ReactFiberWorkLoop.js:3706-3735` |
| `suspense-begin-work` | 3 | 12.51 / 27.95 / 6.14 ms | `packages/react-reconciler/src/ReactFiberBeginWork.js:4367-4389` |
| `delegated-dom-events` | 1 | 10.61 / 10.85 / 0.18 ms | `packages/react-dom-bindings/src/events/DOMPluginEventSystem.js:430-433` |
| `create-dom-root` | 3 | 11.03 / 11.12 / 0.20 ms | `packages/react-dom/src/client/ReactDOMRootFB.js:1-33` |
| `reconcile-keyed-array` | 1 | 10.89 / 11.59 / 0.28 ms | `packages/react-reconciler/src/ReactChildFiber.js:1160-1182` |
| `dispatch-state-update` | 1 | 10.33 / 10.54 / 0.15 ms | `packages/react-reconciler/src/ReactFiberHooks.js:3602-3630` |
| `hydrate-dom-root` | 1 | 9.78 / 9.90 / 0.09 ms | `packages/react-dom/src/client/ReactDOMRootFB.js:134-169` |

## react · Semble · symbol 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `state-hook-api` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/react/src/ReactHooks.js:44-66` |
| `create-context` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/react/src/ReactContext.js:1-18` |
| `schedule-fiber-update` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/react-reconciler/src/ReactFiberWorkLoop.js:972-1005` |
| `commit-root` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/react-reconciler/src/ReactFiberWorkLoop.js:3706-3735` |
| `suspense-begin-work` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/react-reconciler/src/ReactFiberBeginWork.js:2369-2387` |
| `delegated-dom-events` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/react-dom-bindings/src/events/DOMPluginEventSystem.js:430-433` |
| `create-dom-root` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/react-dom/src/client/ReactDOMRoot.js:153-174` |
| `reconcile-keyed-array` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/react-reconciler/src/ReactChildFiber.js:1160-1182` |
| `dispatch-state-update` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/react-reconciler/src/ReactFiberHooks.js:3602-3630` |
| `hydrate-dom-root` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/react-dom/src/client/ReactDOMRootFB.js:134-169` |

## react · CodeGraph · natural_language 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `state-hook-api` | 未命中 | 204.85 / 211.44 / 3.00 ms | `compiler/packages/babel-plugin-react-compiler/src/ReactiveScopes/PruneNonEscapingScopes.ts:202-417` |
| `create-context` | 未命中 | 248.40 / 251.60 / 2.98 ms | `compiler/crates/react_compiler_hir/src/environment_config.rs:1-230` |
| `schedule-fiber-update` | 未命中 | 177.86 / 180.33 / 1.97 ms | `compiler/packages/snap/src/reporter.ts:117-181` |
| `commit-root` | 未命中 | 257.56 / 306.25 / 19.81 ms | `compiler/packages/babel-plugin-react-compiler/src/Utils/Result.ts:115-201` |
| `suspense-begin-work` | 未命中 | 230.27 / 232.95 / 1.73 ms | `compiler/packages/babel-plugin-react-compiler/src/Flood/Types.ts:17-946` |
| `delegated-dom-events` | 未命中 | 274.57 / 275.35 / 1.58 ms | `packages/internal-test-utils/ReactInternalTestUtils.js:349-379` |
| `create-dom-root` | 未命中 | 211.90 / 214.24 / 1.10 ms | `compiler/packages/babel-plugin-react-compiler/src/Inference/InferMutationAliasingRanges.ts:599-619` |
| `reconcile-keyed-array` | 未命中 | 226.78 / 242.64 / 7.47 ms | `compiler/packages/babel-plugin-react-compiler/src/Flood/Types.ts:17-946` |
| `dispatch-state-update` | 未命中 | 183.96 / 203.61 / 9.03 ms | `compiler/packages/snap/src/reporter.ts:1-275` |
| `hydrate-dom-root` | 未命中 | 268.40 / 272.53 / 2.65 ms | `packages/react-dom/src/client/ReactDOMRoot.js:149-357` |

## react · CodeGraph · literal 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `state-hook-api` | 未命中 | 202.23 / 224.65 / 9.12 ms | `compiler/packages/babel-plugin-react-compiler/src/Flood/Types.ts:148-865` |
| `create-context` | 未命中 | 226.69 / 231.59 / 3.10 ms | `compiler/crates/react_compiler_hir/src/object_shape.rs:305-433` |
| `schedule-fiber-update` | 1 | 233.96 / 235.11 / 1.69 ms | `packages/react-reconciler/src/ReactFiberWorkLoop.js:699-1132` |
| `commit-root` | 未命中 | 200.31 / 202.11 / 1.09 ms | `packages/react-dom/src/client/ReactDOMRoot.js:149-357` |
| `suspense-begin-work` | 未命中 | 246.94 / 250.10 / 2.24 ms | `packages/react-reconciler/src/ReactFiberRootScheduler.js:572-582` |
| `delegated-dom-events` | 未命中 | 287.21 / 294.48 / 3.28 ms | `packages/react-dom/src/client/ReactDOMRoot.js:149-357` |
| `create-dom-root` | 未命中 | 248.93 / 250.32 / 1.06 ms | `packages/react-dom/src/client/ReactDOMRoot.js:149-357` |
| `reconcile-keyed-array` | 未命中 | 291.47 / 298.40 / 3.24 ms | `compiler/crates/react_compiler_hir/src/environment.rs:38-534` |
| `dispatch-state-update` | 未命中 | 238.16 / 240.85 / 4.42 ms | `compiler/crates/react_compiler_inference/src/infer_mutation_aliasing_effects.rs:117-144` |
| `hydrate-dom-root` | 未命中 | 211.90 / 216.60 / 2.41 ms | `packages/react-dom-bindings/src/shared/ReactFlightClientConfigDOM.js:1-139` |

## react · CodeGraph · symbol 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `state-hook-api` | 未命中 | 2.51 / 3.14 / 0.29 ms | `compiler/packages/babel-plugin-react-compiler/src/__tests__/fixtures/compiler/globals-dont-resolve-local-useState.js:4-7` |
| `create-context` | 未命中 | 2.78 / 3.19 / 0.19 ms | `compiler/apps/playground/lib/createContext.ts:26-40` |
| `schedule-fiber-update` | 1 | 2.14 / 2.57 / 0.23 ms | `packages/react-reconciler/src/ReactFiberWorkLoop.js:987-1113` |
| `commit-root` | 未命中 | 2.23 / 2.84 / 0.29 ms | `packages/react-reconciler/src/ReactFiberCommitWork.js:220-238` |
| `suspense-begin-work` | 未命中 | 12.34 / 12.72 / 0.27 ms | `—` |
| `delegated-dom-events` | 1 | 2.34 / 2.78 / 0.27 ms | `packages/react-dom-bindings/src/events/DOMPluginEventSystem.js:432-459` |
| `create-dom-root` | 未命中 | 2.68 / 2.94 / 0.26 ms | `packages/use-sync-external-store/src/__tests__/useSyncExternalStoreShared-test.js:90-110` |
| `reconcile-keyed-array` | 未命中 | 14.48 / 15.25 / 0.65 ms | `—` |
| `dispatch-state-update` | 未命中 | 12.23 / 13.05 / 0.40 ms | `—` |
| `hydrate-dom-root` | 未命中 | 2.14 / 2.40 / 0.11 ms | `packages/react-dom/src/__tests__/ReactDOMFizzShellHydration-test.js:104-123` |

## vue · Semble · natural_language 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `reactive-proxy` | 1 | 1.85 / 1.92 / 0.08 ms | `packages/reactivity/src/reactive.ts:85-106` |
| `create-ref` | 4 | 1.87 / 1.89 / 0.02 ms | `packages/reactivity/src/ref.ts:104-114` |
| `create-renderer` | 1 | 1.90 / 1.92 / 0.03 ms | `packages/runtime-core/src/renderer.ts:303-323` |
| `patch-keyed-children` | 3 | 1.62 / 1.76 / 0.06 ms | `packages/runtime-core/src/compat/instanceChildren.ts:1-17` |
| `compile-template` | 未命中 | 1.91 / 1.95 / 0.04 ms | `packages/compiler-sfc/src/compileTemplate.ts:105-107` |
| `parse-template` | 2 | 2.18 / 2.23 / 0.03 ms | `packages/compiler-sfc/src/parse.ts:95-110` |
| `reactivity-watch` | 2 | 1.75 / 1.77 / 0.04 ms | `packages/reactivity/src/index.ts:81-97` |
| `create-app-api` | 2 | 2.05 / 2.09 / 0.06 ms | `packages/runtime-core/src/component.ts:597-624` |
| `define-component` | 1 | 1.92 / 2.01 / 0.04 ms | `packages/runtime-core/src/apiDefineComponent.ts:303-315` |
| `hydrate-node` | 1 | 1.58 / 1.70 / 0.06 ms | `packages/runtime-core/src/hydration.ts:119-137` |

## vue · Semble · literal 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `reactive-proxy` | 1 | 1.75 / 1.87 / 0.05 ms | `packages/reactivity/src/reactive.ts:85-106` |
| `create-ref` | 1 | 1.96 / 1.97 / 0.02 ms | `packages/reactivity/src/ref.ts:52-72` |
| `create-renderer` | 2 | 1.98 / 2.00 / 0.03 ms | `packages/runtime-core/src/renderer.ts:325-346` |
| `patch-keyed-children` | 3 | 2.14 / 2.25 / 0.07 ms | `packages/runtime-core/src/renderer.ts:1660-1675` |
| `compile-template` | 2 | 1.82 / 1.97 / 0.07 ms | `packages/compiler-core/src/index.ts:1-30` |
| `parse-template` | 2 | 1.77 / 1.85 / 0.04 ms | `packages/compiler-core/src/parser.ts:70-96` |
| `reactivity-watch` | 4 | 1.58 / 1.62 / 0.03 ms | `packages/reactivity/src/watch.ts:49-78` |
| `create-app-api` | 1 | 2.12 / 2.28 / 0.07 ms | `packages/runtime-core/src/apiCreateApp.ts:253-273` |
| `define-component` | 1 | 1.91 / 2.00 / 0.05 ms | `packages/runtime-core/src/apiDefineComponent.ts:303-315` |
| `hydrate-node` | 1 | 2.41 / 2.48 / 0.04 ms | `packages/runtime-core/src/hydration.ts:119-137` |

## vue · Semble · symbol 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `reactive-proxy` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/reactivity/src/reactive.ts:85-106` |
| `create-ref` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/reactivity/src/ref.ts:52-72` |
| `create-renderer` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/runtime-core/src/renderer.ts:303-323` |
| `patch-keyed-children` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/runtime-core/src/renderer.ts:1777-1804` |
| `compile-template` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/compiler-core/src/compile.ts:65-70` |
| `parse-template` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/compiler-core/src/parser.ts:1009-1028` |
| `reactivity-watch` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/reactivity/src/watch.ts:103-124` |
| `create-app-api` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/runtime-core/src/apiCreateApp.ts:253-273` |
| `define-component` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/runtime-core/src/apiDefineComponent.ts:303-315` |
| `hydrate-node` | 1 | 0.01 / 0.01 / 0.00 ms | `packages/runtime-core/src/hydration.ts:119-137` |

## vue · CodeGraph · natural_language 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `reactive-proxy` | 1 | 48.76 / 51.18 / 1.03 ms | `packages/reactivity/src/reactive.ts:15-438` |
| `create-ref` | 1 | 46.94 / 48.57 / 0.81 ms | `packages/reactivity/src/ref.ts:61-579` |
| `create-renderer` | 2 | 72.82 / 72.94 / 0.32 ms | `packages/runtime-core/src/compat/global.ts:93-353` |
| `patch-keyed-children` | 未命中 | 50.77 / 51.52 / 0.92 ms | `packages/runtime-core/src/renderer.ts:153-2640` |
| `compile-template` | 未命中 | 56.67 / 58.12 / 0.74 ms | `packages/runtime-core/src/compat/global.ts:146-156` |
| `parse-template` | 未命中 | 49.11 / 49.73 / 0.43 ms | `packages/compiler-sfc/src/parse.ts:22-354` |
| `reactivity-watch` | 未命中 | 47.20 / 47.46 / 0.48 ms | `packages/reactivity/src/watch.ts:34-93` |
| `create-app-api` | 未命中 | 62.82 / 63.58 / 0.45 ms | `packages/runtime-core/src/compat/global.ts:85-156` |
| `define-component` | 2 | 61.96 / 62.26 / 0.18 ms | `packages/runtime-core/src/compat/global.ts:64-333` |
| `hydrate-node` | 未命中 | 61.93 / 63.06 / 0.67 ms | `packages/runtime-core/src/compat/global.ts:146-156` |

## vue · CodeGraph · literal 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `reactive-proxy` | 1 | 43.32 / 44.13 / 0.45 ms | `packages/reactivity/src/reactive.ts:1-446` |
| `create-ref` | 1 | 38.00 / 38.77 / 0.38 ms | `packages/reactivity/src/ref.ts:1-579` |
| `create-renderer` | 未命中 | 50.20 / 50.30 / 0.26 ms | `packages/runtime-core/src/renderer.ts:93-304` |
| `patch-keyed-children` | 未命中 | 41.23 / 41.83 / 0.32 ms | `packages-private/dts-test/ref.test-d.ts:345-351` |
| `compile-template` | 1 | 39.40 / 40.28 / 0.38 ms | `packages/compiler-core/src/compile.ts:1-122` |
| `parse-template` | 未命中 | 40.95 / 41.30 / 0.25 ms | `packages/compiler-core/src/parser.ts:1-288` |
| `reactivity-watch` | 未命中 | 37.55 / 38.09 / 0.54 ms | `packages/runtime-core/src/apiWatch.ts:1-92` |
| `create-app-api` | 1 | 40.47 / 40.94 / 0.46 ms | `packages/runtime-core/src/apiCreateApp.ts:30-494` |
| `define-component` | 未命中 | 43.52 / 45.06 / 0.66 ms | `packages/shared/src/general.ts:1-219` |
| `hydrate-node` | 未命中 | 45.72 / 47.40 / 0.78 ms | `packages/runtime-core/src/vnode.ts:71-256` |

## vue · CodeGraph · symbol 明细

| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |
| --- | ---: | ---: | --- |
| `reactive-proxy` | 1 | 0.88 / 0.91 / 0.02 ms | `packages/reactivity/src/reactive.ts:87-99` |
| `create-ref` | 1 | 0.96 / 1.12 / 0.07 ms | `packages/reactivity/src/ref.ts:64-66` |
| `create-renderer` | 1 | 0.30 / 0.30 / 0.01 ms | `packages/runtime-core/src/renderer.ts:318-323` |
| `patch-keyed-children` | 未命中 | 1.96 / 4.02 / 0.82 ms | `—` |
| `compile-template` | 1 | 0.33 / 0.35 / 0.01 ms | `packages/compiler-core/src/compile.ts:67-122` |
| `parse-template` | 1 | 0.40 / 0.41 / 0.00 ms | `packages/compiler-core/src/parser.ts:1028-1079` |
| `reactivity-watch` | 1 | 0.58 / 0.60 / 0.01 ms | `packages/reactivity/src/watch.ts:120-331` |
| `create-app-api` | 1 | 0.29 / 0.30 / 0.00 ms | `packages/runtime-core/src/apiCreateApp.ts:253-487` |
| `define-component` | 1 | 0.62 / 0.64 / 0.01 ms | `packages/runtime-core/src/apiDefineComponent.ts:305-315` |
| `hydrate-node` | 未命中 | 0.27 / 0.37 / 0.05 ms | `packages/runtime-core/src/components/Suspense.ts:806-856` |

## 公平性说明

自然语言和多词字面轨道调用 CodeGraph 官方推荐的 codegraph_explore，包含图扩展和最终源码读取；符号轨道调用 searchNodes。Semble 三条轨道都调用同一个混合搜索接口。所有查询共享同一组人工标注实现位置，未针对系统选择不同真值。两者输出粒度不同，因此本报告适合比较达到同一代码位置的效果和实际工具延迟，不代表各内部算法的微基准。CodeGraph 的独立 callers、callees 和 impact 能力不属于本次范围。
