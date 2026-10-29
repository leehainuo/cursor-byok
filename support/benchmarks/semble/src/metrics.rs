//! Retrieval metrics and latency summaries used by the benchmark report.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct EffectMetrics {
    pub query_count: usize,
    pub recall_at_1: f64,
    pub recall_at_3: f64,
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub mrr_at_10: f64,
    pub ndcg_at_10: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LatencyMetrics {
    pub samples: usize,
    pub min_ms: f64,
    pub max_ms: f64,
    pub mean_ms: f64,
    pub stddev_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

pub fn effect(ranks: &[Option<usize>]) -> EffectMetrics {
    let count = ranks.len();
    let denominator = count.max(1) as f64;
    let recall = |limit: usize| {
        ranks
            .iter()
            .filter(|rank| rank.is_some_and(|rank| rank <= limit))
            .count() as f64
            / denominator
    };
    let mrr = positive_zero(
        ranks
            .iter()
            .filter_map(|rank| rank.filter(|rank| *rank <= 10))
            .map(|rank| 1.0 / rank as f64)
            .sum::<f64>()
            / denominator,
    );
    let ndcg = positive_zero(
        ranks
            .iter()
            .filter_map(|rank| rank.filter(|rank| *rank <= 10))
            .map(|rank| 1.0 / (rank as f64 + 1.0).log2())
            .sum::<f64>()
            / denominator,
    );
    EffectMetrics {
        query_count: count,
        recall_at_1: recall(1),
        recall_at_3: recall(3),
        recall_at_5: recall(5),
        recall_at_10: recall(10),
        mrr_at_10: mrr,
        ndcg_at_10: ndcg,
    }
}

fn positive_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

pub fn latency(samples_ms: &[f64]) -> LatencyMetrics {
    let mut sorted = samples_ms.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = sorted.iter().sum::<f64>() / sorted.len().max(1) as f64;
    let variance = sorted
        .iter()
        .map(|sample| (sample - mean).powi(2))
        .sum::<f64>()
        / sorted.len().max(1) as f64;
    LatencyMetrics {
        samples: sorted.len(),
        min_ms: sorted.first().copied().unwrap_or_default(),
        max_ms: sorted.last().copied().unwrap_or_default(),
        mean_ms: mean,
        stddev_ms: variance.sqrt(),
        p50_ms: percentile(&sorted, 0.50),
        p95_ms: percentile(&sorted, 0.95),
        p99_ms: percentile(&sorted, 0.99),
    }
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).ceil() as usize;
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_binary_relevance_metrics() {
        let metrics = effect(&[Some(1), Some(4), None]);
        assert!((metrics.recall_at_1 - 1.0 / 3.0).abs() < 1e-9);
        assert!((metrics.recall_at_5 - 2.0 / 3.0).abs() < 1e-9);
        assert!((metrics.mrr_at_10 - (1.0 + 0.25) / 3.0).abs() < 1e-9);
    }

    #[test]
    fn reports_nearest_rank_percentiles() {
        let metrics = latency(&[5.0, 1.0, 3.0, 2.0, 4.0]);
        assert_eq!(metrics.mean_ms, 3.0);
        assert_eq!(metrics.min_ms, 1.0);
        assert_eq!(metrics.max_ms, 5.0);
        assert_eq!(metrics.p50_ms, 3.0);
        assert_eq!(metrics.p95_ms, 5.0);
        assert_eq!(metrics.p99_ms, 5.0);
        assert!((metrics.stddev_ms - 2.0_f64.sqrt()).abs() < 1e-9);
    }
}
