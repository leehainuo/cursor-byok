//! Explicit quality floors for detecting retrieval regressions in CI.

use std::{collections::BTreeMap, fs, path::Path};

use serde::Deserialize;

use crate::report::BenchmarkReport;

const REQUIRED_TRACKS: [&str; 3] = ["natural_language", "literal", "symbol"];

#[derive(Debug, Deserialize)]
struct QualityGates {
    schema_version: u32,
    comparisons: ComparisonGates,
    tracks: BTreeMap<String, QualityGate>,
}

#[derive(Debug, Deserialize)]
struct ComparisonGates {
    cached_load: RatioGate,
    symbol_latency: LatencyRatioGate,
}

#[derive(Debug, Deserialize)]
struct RatioGate {
    max_ratio: f64,
}

#[derive(Debug, Deserialize)]
struct LatencyRatioGate {
    max_p50_ratio: f64,
    max_p95_ratio: f64,
}

#[derive(Debug, Deserialize)]
struct QualityGate {
    recall_at_1: f64,
    recall_at_5: f64,
    recall_at_10: f64,
    mrr_at_10: f64,
    ndcg_at_10: f64,
}

pub fn check(path: &Path, report: &BenchmarkReport) -> Result<(), String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("read quality gates {}: {error}", path.display()))?;
    let gates = serde_json::from_slice::<QualityGates>(&bytes)
        .map_err(|error| format!("parse quality gates {}: {error}", path.display()))?;
    if gates.schema_version != 1 {
        return Err(format!(
            "unsupported quality gate schema {} in {}",
            gates.schema_version,
            path.display()
        ));
    }
    validate(&gates)?;
    let mut failures = Vec::new();
    for (track, gate) in &gates.tracks {
        let Some(result) = report
            .overall
            .iter()
            .find(|result| result.system == "Semble" && result.track == *track)
        else {
            failures.push(format!("missing Semble track {track}"));
            continue;
        };
        let metrics = &result.effect;
        compare(
            &mut failures,
            track,
            "Recall@1",
            metrics.recall_at_1,
            gate.recall_at_1,
        );
        compare(
            &mut failures,
            track,
            "Recall@5",
            metrics.recall_at_5,
            gate.recall_at_5,
        );
        compare(
            &mut failures,
            track,
            "Recall@10",
            metrics.recall_at_10,
            gate.recall_at_10,
        );
        compare(
            &mut failures,
            track,
            "MRR@10",
            metrics.mrr_at_10,
            gate.mrr_at_10,
        );
        compare(
            &mut failures,
            track,
            "nDCG@10",
            metrics.ndcg_at_10,
            gate.ndcg_at_10,
        );
    }
    for suite in &report.suites {
        let Some(semble) = suite
            .systems
            .iter()
            .find(|system| system.system == "Semble")
        else {
            failures.push(format!("{} is missing Semble results", suite.name));
            continue;
        };
        let Some(codegraph) = suite
            .systems
            .iter()
            .find(|system| system.system == "CodeGraph")
        else {
            continue;
        };
        compare_ratio(
            &mut failures,
            &suite.name,
            "cached load",
            semble.index.cached_load_ms,
            codegraph.index.cached_load_ms,
            gates.comparisons.cached_load.max_ratio,
        );
        let Some(semble_symbol) = semble.tracks.iter().find(|track| track.name == "symbol") else {
            failures.push(format!("{} is missing Semble symbol results", suite.name));
            continue;
        };
        let Some(codegraph_symbol) = codegraph.tracks.iter().find(|track| track.name == "symbol")
        else {
            failures.push(format!(
                "{} is missing CodeGraph symbol results",
                suite.name
            ));
            continue;
        };
        compare_latency(
            &mut failures,
            &suite.name,
            "P50",
            semble_symbol.query_latency.p50_ms,
            codegraph_symbol.query_latency.p50_ms,
            gates.comparisons.symbol_latency.max_p50_ratio,
        );
        compare_latency(
            &mut failures,
            &suite.name,
            "P95",
            semble_symbol.query_latency.p95_ms,
            codegraph_symbol.query_latency.p95_ms,
            gates.comparisons.symbol_latency.max_p95_ratio,
        );
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "quality gates failed:\n- {}",
            failures.join("\n- ")
        ))
    }
}

fn validate(gates: &QualityGates) -> Result<(), String> {
    for track in REQUIRED_TRACKS {
        if !gates.tracks.contains_key(track) {
            return Err(format!("quality gates are missing required track {track}"));
        }
    }
    for (track, gate) in &gates.tracks {
        for (metric, value) in [
            ("Recall@1", gate.recall_at_1),
            ("Recall@5", gate.recall_at_5),
            ("Recall@10", gate.recall_at_10),
            ("MRR@10", gate.mrr_at_10),
            ("nDCG@10", gate.ndcg_at_10),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(format!(
                    "quality gate {track} {metric} must be between 0 and 1"
                ));
            }
        }
    }
    for (metric, value) in [
        ("cached load ratio", gates.comparisons.cached_load.max_ratio),
        (
            "symbol latency P50 ratio",
            gates.comparisons.symbol_latency.max_p50_ratio,
        ),
        (
            "symbol latency P95 ratio",
            gates.comparisons.symbol_latency.max_p95_ratio,
        ),
    ] {
        if !value.is_finite() || value <= 0.0 || value > 1.0 {
            return Err(format!(
                "quality gate {metric} must be greater than 0 and at most 1"
            ));
        }
    }
    Ok(())
}

fn compare(failures: &mut Vec<String>, track: &str, metric: &str, actual: f64, minimum: f64) {
    if actual + f64::EPSILON < minimum {
        failures.push(format!(
            "{track} {metric} was {actual:.3}, minimum is {minimum:.3}"
        ));
    }
}

fn compare_latency(
    failures: &mut Vec<String>,
    suite: &str,
    percentile: &str,
    semble_ms: f64,
    codegraph_ms: f64,
    max_ratio: f64,
) {
    compare_ratio(
        failures,
        suite,
        &format!("symbol {percentile}"),
        semble_ms,
        codegraph_ms,
        max_ratio,
    );
}

fn compare_ratio(
    failures: &mut Vec<String>,
    suite: &str,
    metric: &str,
    semble_ms: f64,
    codegraph_ms: f64,
    max_ratio: f64,
) {
    let limit = codegraph_ms * max_ratio;
    if semble_ms > limit {
        failures.push(format!(
            "{suite} {metric} was {semble_ms:.3} ms; limit is {limit:.3} ms ({max_ratio:.0}% of CodeGraph {codegraph_ms:.3} ms)"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(value: f64) -> QualityGate {
        QualityGate {
            recall_at_1: value,
            recall_at_5: value,
            recall_at_10: value,
            mrr_at_10: value,
            ndcg_at_10: value,
        }
    }

    fn comparisons() -> ComparisonGates {
        ComparisonGates {
            cached_load: RatioGate { max_ratio: 0.9 },
            symbol_latency: LatencyRatioGate {
                max_p50_ratio: 0.9,
                max_p95_ratio: 0.9,
            },
        }
    }

    #[test]
    fn rejects_incomplete_and_out_of_range_configuration() {
        let mut tracks = BTreeMap::new();
        tracks.insert("natural_language".into(), gate(0.5));
        let gates = QualityGates {
            schema_version: 1,
            comparisons: comparisons(),
            tracks,
        };
        assert!(validate(&gates).unwrap_err().contains("literal"));

        let mut tracks = REQUIRED_TRACKS
            .into_iter()
            .map(|track| (track.to_owned(), gate(0.5)))
            .collect::<BTreeMap<_, _>>();
        tracks.insert("literal".into(), gate(1.1));
        let gates = QualityGates {
            schema_version: 1,
            comparisons: comparisons(),
            tracks,
        };
        assert!(validate(&gates).unwrap_err().contains("between 0 and 1"));
    }

    #[test]
    fn comparison_only_records_values_below_the_floor() {
        let mut failures = Vec::new();
        compare(&mut failures, "literal", "Recall@1", 0.6, 0.6);
        assert!(failures.is_empty());
        compare(&mut failures, "literal", "Recall@1", 0.59, 0.6);
        assert_eq!(failures.len(), 1);
    }

    #[test]
    fn latency_comparison_enforces_the_configured_margin() {
        let mut failures = Vec::new();
        compare_latency(&mut failures, "react", "P50", 0.8, 1.0, 0.9);
        assert!(failures.is_empty());
        compare_latency(&mut failures, "react", "P95", 0.95, 1.0, 0.9);
        assert_eq!(failures.len(), 1);
    }

    #[test]
    fn cached_load_comparison_enforces_the_configured_margin() {
        let mut failures = Vec::new();
        compare_ratio(&mut failures, "react", "cached load", 80.0, 100.0, 0.9);
        assert!(failures.is_empty());
        compare_ratio(&mut failures, "react", "cached load", 95.0, 100.0, 0.9);
        assert_eq!(failures.len(), 1);
    }
}
