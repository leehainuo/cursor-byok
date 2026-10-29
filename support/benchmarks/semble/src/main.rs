//! Reproducible Semble and CodeGraph comparison on React and Vue.

mod case;
mod codegraph;
mod gates;
mod metrics;
mod report;
mod repository;

use std::{
    collections::{BTreeSet, HashMap},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use case::{ExpectedLocation, QueryCase, Suite};
use codegraph::{AdapterResult, AdapterTrack};
use report::{
    BenchmarkReport, Configuration, Environment, IndexMetrics, OverallReport, QueryReport,
    ResultSummary, SuiteReport, SystemReport, TrackReport,
};
use semble_core::{
    embedding::ModelAssets, ContentType, SearchEngine, SearchRequest, SearchResult, SembleConfig,
};

const TOP_K: usize = 10;
const TRACKS: [&str; 3] = ["natural_language", "literal", "symbol"];

fn main() {
    if let Err(error) = run() {
        eprintln!("semble-benchmark: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse()?;
    if args.help {
        print_help();
        return Ok(());
    }
    let suites = case::load_suites(&args.cases_dir)?;
    let sources = repository::resolve_sources(&suites, &args.sources_dir, &args.sources)?;
    let codegraph_sources = if args.semble_only {
        HashMap::new()
    } else {
        repository::prepare_competitor_sources(&suites, &sources, &args.codegraph_sources_dir)?
    };
    fs::create_dir_all(&args.cache_dir).map_err(|error| {
        format!(
            "create cache directory {}: {error}",
            args.cache_dir.display()
        )
    })?;
    if args.reset_indexes {
        remove_indexes(&args.cache_dir)?;
    }
    ModelAssets::ensure(&args.cache_dir).map_err(|error| error.to_string())?;

    let mut suite_reports = Vec::with_capacity(suites.len());
    for suite in &suites {
        println!("benchmarking {} at {}", suite.name, suite.commit);
        let source = sources
            .get(&suite.name)
            .ok_or_else(|| format!("missing source path for {}", suite.name))?;
        let codegraph_source = codegraph_sources.get(&suite.name).map(PathBuf::as_path);
        suite_reports.push(benchmark_suite(suite, source, codegraph_source, &args)?);
    }

    let report = BenchmarkReport {
        schema_version: 4,
        generated_at_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        environment: Environment {
            os: env::consts::OS.into(),
            architecture: env::consts::ARCH.into(),
            rustc: rustc_version(),
        },
        configuration: Configuration {
            top_k: TOP_K,
            repetitions: args.repetitions,
            tracks: TRACKS.iter().map(|track| (*track).to_owned()).collect(),
            relevance:
                "result path matches and result line range covers a labelled implementation line"
                    .into(),
        },
        overall: overall(&suite_reports),
        suites: suite_reports,
    };
    write_report(&args.json, &args.markdown, &report)?;
    println!("wrote {}", args.json.display());
    println!("wrote {}", args.markdown.display());
    if args.check {
        gates::check(&args.quality_gates, &report)?;
        println!("quality gates passed");
    }
    Ok(())
}

fn benchmark_suite(
    suite: &Suite,
    source: &Path,
    codegraph_source: Option<&Path>,
    args: &Args,
) -> Result<SuiteReport, String> {
    validate_ground_truth(suite, source)?;
    let semble = benchmark_semble(suite, source, &args.cache_dir, args.repetitions)?;
    let mut systems = vec![semble];
    if let Some(codegraph_source) = codegraph_source {
        let codegraph = codegraph::benchmark(
            &args.codegraph,
            &args.codegraph_adapter,
            codegraph_source,
            suite,
            args.repetitions,
            TOP_K,
        )?;
        let cold_seconds = (codegraph.cold_index_ms / 1_000.0).max(f64::EPSILON);
        systems.push(SystemReport {
            system: "CodeGraph".into(),
            version: codegraph.version,
            index: IndexMetrics {
                cold_ready_ms: codegraph.cold_index_ms,
                cold_index_ms: codegraph.cold_index_ms,
                cached_load_ms: codegraph.adapter.load_ms,
                in_memory_prepare_ms: None,
                indexed_files: codegraph.adapter.stats.file_count,
                indexed_units: codegraph.adapter.stats.node_count,
                indexed_unit: "nodes".into(),
                indexed_edges: Some(codegraph.adapter.stats.edge_count),
                persisted_index_bytes: codegraph.persisted_index_bytes,
                source_bytes: None,
                units_per_second: codegraph.adapter.stats.node_count as f64 / cold_seconds,
                source_mib_per_second: None,
            },
            refresh_symbol_latency: None,
            tracks: codegraph
                .adapter
                .tracks
                .iter()
                .map(|track| codegraph_track(track, suite))
                .collect::<Result<Vec<_>, _>>()?,
        });
    }
    Ok(SuiteReport {
        name: suite.name.clone(),
        repository: suite.repository.clone(),
        commit: suite.commit.clone(),
        systems,
    })
}

fn benchmark_semble(
    suite: &Suite,
    source: &Path,
    cache_dir: &Path,
    repetitions: usize,
) -> Result<SystemReport, String> {
    let index_bytes_before = directory_size(&cache_dir.join("indexes"))?;
    let model_started = Instant::now();
    let engine = SearchEngine::load_default(SembleConfig::new(cache_dir))
        .map_err(|error| error.to_string())?;
    let model_load_ms = elapsed_ms(model_started.elapsed());
    let cold_started = Instant::now();
    let stats = engine
        .prepare(source, &[ContentType::Code])
        .map_err(|error| error.to_string())?;
    let cold_duration = cold_started.elapsed();
    let memory_started = Instant::now();
    let memory_stats = engine
        .prepare(source, &[ContentType::Code])
        .map_err(|error| error.to_string())?;
    let in_memory_prepare_ms = elapsed_ms(memory_started.elapsed());
    if memory_stats != stats {
        return Err(format!("{} in-memory index stats changed", suite.name));
    }
    drop(engine);

    let reload_started = Instant::now();
    let engine = SearchEngine::load_default(SembleConfig::new(cache_dir))
        .map_err(|error| error.to_string())?;
    let model_reload_ms = elapsed_ms(reload_started.elapsed());
    let disk_started = Instant::now();
    let disk_stats = engine
        .prepare(source, &[ContentType::Code])
        .map_err(|error| error.to_string())?;
    let disk_index_load_ms = elapsed_ms(disk_started.elapsed());
    if disk_stats != stats {
        return Err(format!("{} persisted index stats changed", suite.name));
    }
    let tracks = vec![
        semble_track(
            &engine,
            source,
            suite,
            "natural_language",
            |query| &query.query,
            repetitions,
        )?,
        semble_track(
            &engine,
            source,
            suite,
            "literal",
            |query| &query.literal_query,
            repetitions,
        )?,
        semble_track(
            &engine,
            source,
            suite,
            "symbol",
            |query| &query.symbol_query,
            repetitions,
        )?,
    ];
    let refresh_symbol_latency = benchmark_refresh_symbol(&engine, source, suite, repetitions)?;
    let index_bytes_after = directory_size(&cache_dir.join("indexes"))?;
    let cold_seconds = cold_duration.as_secs_f64().max(f64::EPSILON);
    Ok(SystemReport {
        system: "Semble".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        index: IndexMetrics {
            cold_ready_ms: model_load_ms + elapsed_ms(cold_duration),
            cold_index_ms: elapsed_ms(cold_duration),
            cached_load_ms: model_reload_ms + disk_index_load_ms,
            in_memory_prepare_ms: Some(in_memory_prepare_ms),
            indexed_files: stats.file_count,
            indexed_units: stats.chunk_count,
            indexed_unit: "chunks".into(),
            indexed_edges: None,
            persisted_index_bytes: index_bytes_after.saturating_sub(index_bytes_before),
            source_bytes: Some(stats.source_bytes),
            units_per_second: stats.chunk_count as f64 / cold_seconds,
            source_mib_per_second: Some(stats.source_bytes as f64 / 1_048_576.0 / cold_seconds),
        },
        refresh_symbol_latency: Some(refresh_symbol_latency),
        tracks,
    })
}

fn benchmark_refresh_symbol(
    engine: &SearchEngine,
    source: &Path,
    suite: &Suite,
    repetitions: usize,
) -> Result<metrics::LatencyMetrics, String> {
    let mut samples = Vec::with_capacity(suite.queries.len() * repetitions);
    for case in &suite.queries {
        for _ in 0..repetitions {
            let started = Instant::now();
            engine
                .refresh(source, &[ContentType::Code])
                .map_err(|error| format!("refresh before symbol query {}: {error}", case.id))?;
            engine
                .search(SearchRequest {
                    query: case.symbol_query.clone(),
                    repo: source.to_path_buf(),
                    top_k: TOP_K,
                    max_snippet_lines: Some(0),
                    content: vec![ContentType::Code],
                })
                .map_err(|error| format!("symbol query after refresh {}: {error}", case.id))?;
            samples.push(elapsed_ms(started.elapsed()));
        }
    }
    Ok(metrics::latency(&samples))
}

fn validate_ground_truth(suite: &Suite, source: &Path) -> Result<(), String> {
    for case in &suite.queries {
        for expected in &case.expected {
            let path = source.join(&expected.path);
            let text = fs::read_to_string(&path).map_err(|error| {
                format!(
                    "ground truth {} / {} cannot read {}: {error}",
                    suite.name,
                    case.id,
                    path.display()
                )
            })?;
            let lines = text.lines().count();
            if expected.line > lines {
                return Err(format!(
                    "ground truth {} / {} points past {} lines in {}",
                    suite.name, case.id, lines, expected.path
                ));
            }
        }
    }
    Ok(())
}

fn semble_track(
    engine: &SearchEngine,
    source: &Path,
    suite: &Suite,
    name: &str,
    query_text: fn(&QueryCase) -> &str,
    repetitions: usize,
) -> Result<TrackReport, String> {
    let mut queries = Vec::with_capacity(suite.queries.len());
    let mut samples = Vec::with_capacity(suite.queries.len() * repetitions);
    for case in &suite.queries {
        let (report, durations) =
            benchmark_semble_query(engine, source, case, query_text(case), repetitions)?;
        queries.push(report);
        samples.extend(durations);
    }
    Ok(track_report(name, queries, samples))
}

fn benchmark_semble_query(
    engine: &SearchEngine,
    source: &Path,
    case: &QueryCase,
    query: &str,
    repetitions: usize,
) -> Result<(QueryReport, Vec<f64>), String> {
    let mut samples = Vec::with_capacity(repetitions);
    let first_results = engine
        .search(SearchRequest {
            query: query.to_owned(),
            repo: source.to_path_buf(),
            top_k: TOP_K,
            max_snippet_lines: Some(0),
            content: vec![ContentType::Code],
        })
        .map_err(|error| format!("warm query {}: {error}", case.id))?
        .results;
    for _ in 0..repetitions {
        let started = Instant::now();
        engine
            .search(SearchRequest {
                query: query.to_owned(),
                repo: source.to_path_buf(),
                top_k: TOP_K,
                max_snippet_lines: Some(0),
                content: vec![ContentType::Code],
            })
            .map_err(|error| format!("query {}: {error}", case.id))?;
        samples.push(elapsed_ms(started.elapsed()));
    }
    let summaries = first_results
        .iter()
        .enumerate()
        .map(|(index, result)| summarize_semble(index + 1, result, &case.expected))
        .collect::<Vec<_>>();
    Ok((query_report(case, query, summaries, &samples), samples))
}

fn codegraph_track(track: &AdapterTrack, suite: &Suite) -> Result<TrackReport, String> {
    let mut samples = Vec::new();
    let mut queries = Vec::with_capacity(track.queries.len());
    for query in &track.queries {
        let case = suite
            .queries
            .iter()
            .find(|case| case.id == query.id)
            .ok_or_else(|| format!("CodeGraph returned unknown query id {}", query.id))?;
        let summaries = query
            .results
            .iter()
            .enumerate()
            .map(|(index, result)| summarize_codegraph(index + 1, result, &case.expected))
            .collect();
        queries.push(query_report(
            case,
            &query.query,
            summaries,
            &query.durations_ms,
        ));
        samples.extend(query.durations_ms.iter().copied());
    }
    Ok(track_report(&track.name, queries, samples))
}

fn track_report(name: &str, queries: Vec<QueryReport>, samples: Vec<f64>) -> TrackReport {
    let ranks = queries
        .iter()
        .map(|query| query.first_relevant_rank)
        .collect::<Vec<_>>();
    TrackReport {
        name: name.into(),
        effect: metrics::effect(&ranks),
        query_latency: metrics::latency(&samples),
        queries,
    }
}

fn query_report(
    case: &QueryCase,
    query: &str,
    results: Vec<ResultSummary>,
    samples: &[f64],
) -> QueryReport {
    QueryReport {
        id: case.id.clone(),
        query: query.into(),
        first_relevant_rank: results
            .iter()
            .find(|result| result.relevant)
            .map(|result| result.rank),
        latency: metrics::latency(samples),
        expected: case
            .expected
            .iter()
            .map(|location| format!("{}:{}", location.path, location.line))
            .collect(),
        results,
    }
}

fn summarize_semble(
    rank: usize,
    result: &SearchResult,
    expected: &[ExpectedLocation],
) -> ResultSummary {
    summary(
        rank,
        &result.file_path,
        result.start_line,
        result.end_line,
        result.score,
        expected,
    )
}

fn summarize_codegraph(
    rank: usize,
    result: &AdapterResult,
    expected: &[ExpectedLocation],
) -> ResultSummary {
    let mut output = summary(
        rank,
        &result.path,
        result.start_line,
        result.end_line,
        result.score,
        expected,
    );
    if let Some(lines) = result.lines.as_ref() {
        output.relevant = expected
            .iter()
            .any(|location| location.path == result.path && lines.contains(&location.line));
    }
    output
}

fn summary(
    rank: usize,
    path: &str,
    start_line: usize,
    end_line: usize,
    score: f32,
    expected: &[ExpectedLocation],
) -> ResultSummary {
    ResultSummary {
        rank,
        path: path.into(),
        start_line,
        end_line,
        score,
        relevant: expected.iter().any(|location| {
            location.path == path && start_line <= location.line && location.line <= end_line
        }),
    }
}

fn overall(suites: &[SuiteReport]) -> Vec<OverallReport> {
    let systems = suites
        .iter()
        .flat_map(|suite| suite.systems.iter().map(|system| system.system.as_str()))
        .collect::<BTreeSet<_>>();
    systems
        .into_iter()
        .flat_map(|system_name| {
            TRACKS.into_iter().map(move |track_name| {
                let ranks = suites
                    .iter()
                    .filter_map(|suite| {
                        suite
                            .systems
                            .iter()
                            .find(|system| system.system == system_name)
                    })
                    .filter_map(|system| {
                        system.tracks.iter().find(|track| track.name == track_name)
                    })
                    .flat_map(|track| track.queries.iter().map(|query| query.first_relevant_rank))
                    .collect::<Vec<_>>();
                OverallReport {
                    system: system_name.into(),
                    track: track_name.into(),
                    effect: metrics::effect(&ranks),
                }
            })
        })
        .collect()
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn directory_size(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0_u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("read {}: {error}", directory.display()))?
        {
            let entry = entry.map_err(|error| format!("read directory entry: {error}"))?;
            let metadata = entry
                .metadata()
                .map_err(|error| format!("inspect {}: {error}", entry.path().display()))?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Ok(total)
}

fn remove_indexes(cache_dir: &Path) -> Result<(), String> {
    let indexes = cache_dir.join("indexes");
    if indexes.file_name().and_then(|name| name.to_str()) != Some("indexes") {
        return Err(format!(
            "refusing to remove unexpected path {}",
            indexes.display()
        ));
    }
    if indexes.exists() {
        fs::remove_dir_all(&indexes)
            .map_err(|error| format!("reset benchmark indexes {}: {error}", indexes.display()))?;
    }
    Ok(())
}

fn write_report(json: &Path, markdown: &Path, report: &BenchmarkReport) -> Result<(), String> {
    for path in [json, markdown] {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| {
                format!("create report directory {}: {error}", parent.display())
            })?;
        }
    }
    let encoded = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("serialize benchmark report: {error}"))?;
    fs::write(json, encoded).map_err(|error| format!("write {}: {error}", json.display()))?;
    fs::write(markdown, report::markdown(report))
        .map_err(|error| format!("write {}: {error}", markdown.display()))
}

fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into())
}

struct Args {
    cases_dir: PathBuf,
    sources_dir: PathBuf,
    codegraph_sources_dir: PathBuf,
    cache_dir: PathBuf,
    json: PathBuf,
    markdown: PathBuf,
    codegraph_adapter: PathBuf,
    codegraph: String,
    quality_gates: PathBuf,
    sources: HashMap<String, PathBuf>,
    repetitions: usize,
    reset_indexes: bool,
    check: bool,
    semble_only: bool,
    help: bool,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let root =
            env::current_dir().map_err(|error| format!("read current directory: {error}"))?;
        let mut parsed = Self {
            cases_dir: root.join("support/benchmarks/semble/cases"),
            sources_dir: root.join("target/semble-benchmark-sources"),
            codegraph_sources_dir: root.join("target/semble-benchmark-codegraph-sources"),
            cache_dir: root.join("target/semble-benchmark-cache"),
            json: root.join("support/benchmarks/semble/results/latest.json"),
            markdown: root.join("support/benchmarks/semble/results/latest.md"),
            codegraph_adapter: root.join("support/benchmarks/semble/codegraph_adapter.cjs"),
            codegraph: "codegraph".into(),
            quality_gates: root.join("support/benchmarks/semble/quality-gates.json"),
            sources: HashMap::new(),
            repetitions: 5,
            reset_indexes: true,
            check: false,
            semble_only: false,
            help: false,
        };
        let mut arguments = env::args().skip(1);
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--cases-dir" => parsed.cases_dir = value(&mut arguments, &argument)?.into(),
                "--sources-dir" => parsed.sources_dir = value(&mut arguments, &argument)?.into(),
                "--codegraph-sources-dir" => {
                    parsed.codegraph_sources_dir = value(&mut arguments, &argument)?.into()
                }
                "--cache-dir" => parsed.cache_dir = value(&mut arguments, &argument)?.into(),
                "--json" => parsed.json = value(&mut arguments, &argument)?.into(),
                "--markdown" => parsed.markdown = value(&mut arguments, &argument)?.into(),
                "--codegraph" => parsed.codegraph = value(&mut arguments, &argument)?,
                "--codegraph-adapter" => {
                    parsed.codegraph_adapter = value(&mut arguments, &argument)?.into()
                }
                "--quality-gates" => {
                    parsed.quality_gates = value(&mut arguments, &argument)?.into()
                }
                "--source" => {
                    let source = value(&mut arguments, &argument)?;
                    let (name, path) = source
                        .split_once('=')
                        .ok_or_else(|| "--source must be NAME=PATH".to_owned())?;
                    parsed.sources.insert(name.into(), path.into());
                }
                "--repetitions" => {
                    parsed.repetitions = value(&mut arguments, &argument)?
                        .parse()
                        .map_err(|_| "--repetitions must be a positive integer".to_owned())?;
                }
                "--keep-indexes" => parsed.reset_indexes = false,
                "--check" => parsed.check = true,
                "--semble-only" => parsed.semble_only = true,
                "-h" | "--help" => parsed.help = true,
                _ => return Err(format!("unknown argument {argument}")),
            }
        }
        if parsed.repetitions == 0 {
            return Err("--repetitions must be greater than zero".into());
        }
        Ok(parsed)
    }
}

fn value(arguments: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn print_help() {
    println!(
        "Semble and CodeGraph React/Vue benchmark\n\n\
         Options:\n\
           --cases-dir PATH       benchmark suite JSON directory\n\
           --sources-dir PATH     managed pinned Git checkouts\n\
           --codegraph-sources-dir PATH  isolated CodeGraph Git worktrees\n\
           --cache-dir PATH       isolated Semble model and index cache\n\
           --codegraph PATH       CodeGraph executable (default: codegraph)\n\
           --codegraph-adapter PATH  CodeGraph JSON adapter script\n\
           --quality-gates PATH   retrieval quality floor configuration\n\
           --source NAME=PATH     use an existing exact-commit checkout\n\
           --repetitions N        warm search repetitions per query (default 5)\n\
           --json PATH            JSON report path\n\
           --markdown PATH        Markdown report path\n\
           --keep-indexes         keep the existing Semble indexes\n\
           --check                fail when Semble misses a quality floor\n\
           --semble-only          skip CodeGraph for fast Semble iteration"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevance_requires_path_and_line_overlap() {
        let expected = [ExpectedLocation {
            path: "src/lib.rs".into(),
            line: 15,
        }];
        assert!(summary(1, "src/lib.rs", 10, 20, 1.0, &expected).relevant);
        assert!(!summary(1, "src/lib.rs", 16, 20, 1.0, &expected).relevant);
        assert!(!summary(1, "src/other.rs", 10, 20, 1.0, &expected).relevant);
    }

    #[test]
    fn directory_size_sums_nested_files() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("nested")).unwrap();
        fs::write(directory.path().join("one"), [1_u8, 2]).unwrap();
        fs::write(directory.path().join("nested/two"), [3_u8, 4, 5]).unwrap();
        assert_eq!(directory_size(directory.path()).unwrap(), 5);
    }
}
