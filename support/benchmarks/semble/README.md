<!-- Reproducible benchmark protocol for Semble's React and Vue evaluation. -->
# Semble / CodeGraph React/Vue 对比基准

这个基准在相同的 React、Vue 提交上比较 Semble 和本机安装的 CodeGraph。每条查询都人工标注到具体实现行，只有返回代码范围覆盖该行才算命中。

为了兼顾不同检索方式，效果分为三条轨道：

- `natural_language`：Semble `search` 对比 CodeGraph 官方推荐的 `codegraph_explore`；
- `literal`：多词代码片段、错误文本或相邻标识符组合，两边均通过各自的产品级搜索入口检索；
- `symbol`：Semble `search` 对比 CodeGraph `searchNodes`，输入相同精确符号名。

三条轨道复用相同的人工标注实现位置。每次运行还会检查标注文件存在且行号没有越界，避免失效真值产生虚假分数。

## 指标

性能包含：

- 冷启动到可查询、冷索引、索引单元吞吐；
- 持久索引加载时间；
- 缓存热查询 min、max、mean、标准差、P50、P95、P99；
- Semble 显式刷新索引后执行符号查询的独立延迟；
- 索引文件、代码块或图节点、图边、持久化索引体积。

效果包含：

- Recall@1、Recall@3、Recall@5、Recall@10；
- MRR@10；
- nDCG@10；
- 每条查询的首个命中排名和 Top 10 结果明细。

## 运行

需要本机 `PATH` 中存在 `codegraph`。从工作区根目录运行：

```sh
cargo run --release -p semble-benchmark
```

需要用于 CI 或本地回归检查时增加 `--check`。跑分报告仍会正常生成，但任一 Semble 轨道低于 [`quality-gates.json`](./quality-gates.json) 中的公开质量下限时，命令会返回失败。完整双系统报告还要求 Semble 的缓存加载，以及符号查询的 P50、P95，均不超过 CodeGraph 的 90%：

```sh
cargo run --release -p semble-benchmark -- --check
```

只验证 Semble 查询效果和质量门槛、跳过耗时较长的 CodeGraph 重建时使用：

```sh
cargo run --release -p semble-benchmark -- --semble-only --check
```

工具默认自行拉取固定提交，并清空隔离的基准索引，模型缓存保留。CodeGraph 使用位于 `target` 下的独立 Git 工作副本，不会创建、覆盖或删除传入代码库中的 `.codegraph`。已有精确提交的工作区可以复用：

```sh
cargo run --release -p semble-benchmark -- \
  --source react=/path/to/react \
  --source vue=/path/to/vue \
  --repetitions 5
```

结果写入 `results/latest.json` 和 `results/latest.md`。JSON 用于后续回归比较，Markdown 用于人工审阅。运行时环境、提交、参数和逐查询结果都会写进报告。

## 解读限制

这是代码定位基准，不评价生成答案本身。`codegraph_explore` 的图扩展和源码读取计入实际工具耗时，因此这是产品路径对比，不是内部算法微基准。Semble 同时持久化增量构建快照与可直接反序列化的运行时索引；加载后在固定一秒刷新窗口内复用已检查的内存索引，窗口到期后的首次查询或显式 `refresh` 会并行扫描文件元数据，并且只重新处理变化文件。报告将缓存查询与刷新加符号查询分开记录。CodeGraph 的加载指标打开已准备图数据库。CodeGraph 的独立 callers、callees、impact 能力不在本次范围内。标注集规模较小，适合防止检索质量回退，不应被解释为覆盖所有 React/Vue 开发问题的总体准确率。每条查询先执行一次不计时预热，再记录五次；报告中的标准差用于识别抖动，但跨版本性能比较仍应在同一机器、同一电源状态下至少重复三轮。
