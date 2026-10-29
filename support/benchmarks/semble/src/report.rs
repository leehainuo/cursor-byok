//! Machine-readable comparison report structures and Markdown rendering.

use serde::Serialize;

use crate::metrics::{EffectMetrics, LatencyMetrics};

#[derive(Debug, Serialize)]
pub struct BenchmarkReport {
    pub schema_version: u32,
    pub generated_at_unix_seconds: u64,
    pub environment: Environment,
    pub configuration: Configuration,
    pub overall: Vec<OverallReport>,
    pub suites: Vec<SuiteReport>,
}

#[derive(Debug, Serialize)]
pub struct Environment {
    pub os: String,
    pub architecture: String,
    pub rustc: String,
}

#[derive(Debug, Serialize)]
pub struct Configuration {
    pub top_k: usize,
    pub repetitions: usize,
    pub tracks: Vec<String>,
    pub relevance: String,
}

#[derive(Debug, Serialize)]
pub struct OverallReport {
    pub system: String,
    pub track: String,
    pub effect: EffectMetrics,
}

#[derive(Debug, Serialize)]
pub struct SuiteReport {
    pub name: String,
    pub repository: String,
    pub commit: String,
    pub systems: Vec<SystemReport>,
}

#[derive(Debug, Serialize)]
pub struct SystemReport {
    pub system: String,
    pub version: String,
    pub index: IndexMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_symbol_latency: Option<LatencyMetrics>,
    pub tracks: Vec<TrackReport>,
}

#[derive(Debug, Serialize)]
pub struct IndexMetrics {
    pub cold_ready_ms: f64,
    pub cold_index_ms: f64,
    pub cached_load_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_memory_prepare_ms: Option<f64>,
    pub indexed_files: usize,
    pub indexed_units: usize,
    pub indexed_unit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_edges: Option<usize>,
    pub persisted_index_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_bytes: Option<u64>,
    pub units_per_second: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_mib_per_second: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct TrackReport {
    pub name: String,
    pub effect: EffectMetrics,
    pub query_latency: LatencyMetrics,
    pub queries: Vec<QueryReport>,
}

#[derive(Debug, Serialize)]
pub struct QueryReport {
    pub id: String,
    pub query: String,
    pub first_relevant_rank: Option<usize>,
    pub latency: LatencyMetrics,
    pub expected: Vec<String>,
    pub results: Vec<ResultSummary>,
}

#[derive(Debug, Serialize)]
pub struct ResultSummary {
    pub rank: usize,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub score: f32,
    pub relevant: bool,
}

pub fn markdown(report: &BenchmarkReport) -> String {
    let mut output = String::new();
    output.push_str("# Semble 与 CodeGraph React/Vue 对比基准\n\n");
    output.push_str(&format!(
        "- 环境：{} {}，{}\n- 查询：Top {}，每条重复 {} 次\n- natural_language：英文行为描述；literal：多词代码/错误字面片段；symbol：精确符号名\n- 对比：natural_language、literal 使用 CodeGraph explore；symbol 使用 CodeGraph searchNodes\n- 判定：返回代码范围必须覆盖人工标注的实现行\n\n",
        report.environment.os,
        report.environment.architecture,
        report.environment.rustc,
        report.configuration.top_k,
        report.configuration.repetitions
    ));
    output.push_str("## 整体效果\n\n");
    output.push_str("| 系统 | 查询轨道 | Recall@1 | Recall@5 | Recall@10 | MRR@10 | nDCG@10 |\n");
    output.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: |\n");
    for item in &report.overall {
        output.push_str(&format!(
            "| {} | {} | {:.1}% | {:.1}% | {:.1}% | {:.3} | {:.3} |\n",
            item.system,
            item.track,
            item.effect.recall_at_1 * 100.0,
            item.effect.recall_at_5 * 100.0,
            item.effect.recall_at_10 * 100.0,
            item.effect.mrr_at_10,
            item.effect.ndcg_at_10,
        ));
    }
    output.push_str("\n## 性能对比\n\n");
    output.push_str("冷启动就绪包含运行时加载和首次索引；Semble 查询在一秒刷新窗口内直接复用已检查索引，窗口到期或显式 refresh 时重新校验源码指纹。缓存查询与 refresh＋symbol 分开计时；查询耗时均为持久进程中的实际工具处理耗时，不含 CLI 进程启动。每条查询先预热一次。\n\n");
    output.push_str("| 数据集 | 系统 | 冷启动就绪 | 缓存加载 | 自然语言 P50 / P95 / σ | 字面 P50 / P95 / σ | 缓存符号 P50 / P95 / σ | 刷新＋符号 P50 / P95 | 文件 / 索引单元 | 索引体积 |\n");
    output.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for suite in &report.suites {
        for system in &suite.systems {
            let natural = track(system, "natural_language");
            let literal = track(system, "literal");
            let symbol = track(system, "symbol");
            output.push_str(&format!(
                "| {} | {} {} | {:.1} ms | {:.1} ms | {:.2} / {:.2} / {:.2} ms | {:.2} / {:.2} / {:.2} ms | {:.2} / {:.2} / {:.2} ms | {} | {} / {} {}{} | {:.2} MiB |\n",
                suite.name,
                system.system,
                system.version,
                system.index.cold_ready_ms,
                system.index.cached_load_ms,
                natural.query_latency.p50_ms,
                natural.query_latency.p95_ms,
                natural.query_latency.stddev_ms,
                literal.query_latency.p50_ms,
                literal.query_latency.p95_ms,
                literal.query_latency.stddev_ms,
                symbol.query_latency.p50_ms,
                symbol.query_latency.p95_ms,
                symbol.query_latency.stddev_ms,
                system
                    .refresh_symbol_latency
                    .as_ref()
                    .map(|latency| format!("{:.2} / {:.2} ms", latency.p50_ms, latency.p95_ms))
                    .unwrap_or_else(|| "—".into()),
                system.index.indexed_files,
                system.index.indexed_units,
                system.index.indexed_unit,
                system
                    .index
                    .indexed_edges
                    .map(|edges| format!(" / {edges} edges"))
                    .unwrap_or_default(),
                as_mib(system.index.persisted_index_bytes),
            ));
        }
    }
    output.push_str("\n## 分数据集效果\n\n");
    output.push_str("| 数据集 | 系统 | 轨道 | Recall@1 / @5 / @10 | MRR@10 | nDCG@10 |\n");
    output.push_str("| --- | --- | --- | ---: | ---: | ---: |\n");
    for suite in &report.suites {
        for system in &suite.systems {
            for track in &system.tracks {
                output.push_str(&format!(
                    "| {} | {} | {} | {:.1}% / {:.1}% / {:.1}% | {:.3} | {:.3} |\n",
                    suite.name,
                    system.system,
                    track.name,
                    track.effect.recall_at_1 * 100.0,
                    track.effect.recall_at_5 * 100.0,
                    track.effect.recall_at_10 * 100.0,
                    track.effect.mrr_at_10,
                    track.effect.ndcg_at_10,
                ));
            }
        }
    }
    for suite in &report.suites {
        for system in &suite.systems {
            for track in &system.tracks {
                output.push_str(&format!(
                    "\n## {} · {} · {} 明细\n\n",
                    suite.name, system.system, track.name
                ));
                output.push_str("| 查询 | 首个命中 | P50 / P95 / σ | Top 1 |\n");
                output.push_str("| --- | ---: | ---: | --- |\n");
                for query in &track.queries {
                    let rank = query
                        .first_relevant_rank
                        .map(|rank| rank.to_string())
                        .unwrap_or_else(|| "未命中".into());
                    let top = query
                        .results
                        .first()
                        .map(|result| {
                            format!("{}:{}-{}", result.path, result.start_line, result.end_line)
                        })
                        .unwrap_or_else(|| "—".into());
                    output.push_str(&format!(
                        "| `{}` | {} | {:.2} / {:.2} / {:.2} ms | `{}` |\n",
                        query.id,
                        rank,
                        query.latency.p50_ms,
                        query.latency.p95_ms,
                        query.latency.stddev_ms,
                        top
                    ));
                }
            }
        }
    }
    output.push_str("\n## 公平性说明\n\n");
    output.push_str("自然语言和多词字面轨道调用 CodeGraph 官方推荐的 codegraph_explore，包含图扩展和最终源码读取；符号轨道调用 searchNodes。Semble 三条轨道都调用同一个混合搜索接口。所有查询共享同一组人工标注实现位置，未针对系统选择不同真值。两者输出粒度不同，因此本报告适合比较达到同一代码位置的效果和实际工具延迟，不代表各内部算法的微基准。CodeGraph 的独立 callers、callees 和 impact 能力不属于本次范围。\n");
    output
}

fn track<'a>(system: &'a SystemReport, name: &str) -> &'a TrackReport {
    system
        .tracks
        .iter()
        .find(|track| track.name == name)
        .expect("every system report contains all benchmark tracks")
}

fn as_mib(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect() -> EffectMetrics {
        EffectMetrics {
            query_count: 1,
            recall_at_1: 1.0,
            recall_at_3: 1.0,
            recall_at_5: 1.0,
            recall_at_10: 1.0,
            mrr_at_10: 1.0,
            ndcg_at_10: 1.0,
        }
    }

    fn track_report(name: &str) -> TrackReport {
        TrackReport {
            name: name.into(),
            effect: effect(),
            query_latency: LatencyMetrics {
                samples: 1,
                min_ms: 1.0,
                max_ms: 1.0,
                mean_ms: 1.0,
                stddev_ms: 0.0,
                p50_ms: 1.0,
                p95_ms: 1.0,
                p99_ms: 1.0,
            },
            queries: Vec::new(),
        }
    }

    #[test]
    fn markdown_contains_both_systems_and_tracks() {
        let report = BenchmarkReport {
            schema_version: 4,
            generated_at_unix_seconds: 0,
            environment: Environment {
                os: "test".into(),
                architecture: "test".into(),
                rustc: "rustc test".into(),
            },
            configuration: Configuration {
                top_k: 10,
                repetitions: 3,
                tracks: vec!["natural_language".into(), "literal".into(), "symbol".into()],
                relevance: "location overlap".into(),
            },
            overall: vec![OverallReport {
                system: "Semble".into(),
                track: "natural_language".into(),
                effect: effect(),
            }],
            suites: vec![SuiteReport {
                name: "fixture".into(),
                repository: "repo".into(),
                commit: "commit".into(),
                systems: vec![SystemReport {
                    system: "Semble".into(),
                    version: "0.1".into(),
                    index: IndexMetrics {
                        cold_ready_ms: 1.0,
                        cold_index_ms: 1.0,
                        cached_load_ms: 1.0,
                        in_memory_prepare_ms: Some(1.0),
                        indexed_files: 1,
                        indexed_units: 2,
                        indexed_unit: "chunks".into(),
                        indexed_edges: None,
                        persisted_index_bytes: 3,
                        source_bytes: Some(4),
                        units_per_second: 5.0,
                        source_mib_per_second: Some(6.0),
                    },
                    refresh_symbol_latency: Some(LatencyMetrics {
                        samples: 1,
                        min_ms: 2.0,
                        max_ms: 2.0,
                        mean_ms: 2.0,
                        stddev_ms: 0.0,
                        p50_ms: 2.0,
                        p95_ms: 2.0,
                        p99_ms: 2.0,
                    }),
                    tracks: vec![
                        track_report("natural_language"),
                        track_report("literal"),
                        track_report("symbol"),
                    ],
                }],
            }],
        };
        let rendered = markdown(&report);
        assert!(rendered.contains("Semble"));
        assert!(rendered.contains("natural_language"));
    }
}
