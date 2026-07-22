use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pulse::canonical_json::to_canonical_bytes;
use pulse::docs::{
    build_index, current_generation, query_lexical_index, tokenize_query_text, DocsRegistry,
    DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord, DocumentScope,
    IndexOptions, RetrievalConfig, ReviewPolicy, DOCUMENT_SCHEMA,
};
use serde::Serialize;

const DEFAULT_CORPUS_SIZES: &[usize] = &[10, 100, 1_000];
const DEFAULT_FULL_BUILD_SAMPLES: usize = 5;
const DEFAULT_WARM_SEARCH_SAMPLES: usize = 100;
const DEFAULT_INCREMENTAL_SAMPLES: usize = 5;

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    benchmark: &'static str,
    profile: String,
    platform: Platform,
    thresholds: Thresholds,
    scenarios: Vec<ScenarioReport>,
    passed: bool,
}

#[derive(Debug, Serialize)]
struct Platform {
    os: String,
    arch: String,
    rustc: String,
}

#[derive(Debug, Serialize)]
struct Thresholds {
    reference_documents: usize,
    warm_search_p95_ms: f64,
    full_build_p95_ms: f64,
    incremental_refresh_p95_ms: f64,
    max_cache_source_ratio: f64,
}

#[derive(Debug, Serialize)]
struct ScenarioReport {
    documents: usize,
    source_bytes: u64,
    cache_bytes: u64,
    cache_source_ratio: f64,
    full_build: TimingSummary,
    warm_search: TimingSummary,
    incremental_refresh: TimingSummary,
    changed_documents: Vec<u32>,
    passed: bool,
    failures: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TimingSummary {
    samples: usize,
    min_ms: f64,
    median_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

#[derive(Clone, Copy)]
struct Profile {
    name: &'static str,
    corpus_sizes: &'static [usize],
    full_build_samples: usize,
    warm_search_samples: usize,
    incremental_samples: usize,
    enforce_thresholds: bool,
}

fn main() {
    let args = env::args()
        .skip(1)
        .filter(|arg| arg != "--bench")
        .collect::<Vec<_>>();
    let profile = match args.as_slice() {
        [arg] if arg == "--smoke" => Profile {
            name: "smoke",
            corpus_sizes: &[10],
            full_build_samples: 1,
            warm_search_samples: 5,
            incremental_samples: 1,
            enforce_thresholds: false,
        },
        [arg, ..] => {
            eprintln!("unknown argument {arg:?}; supported: --smoke");
            std::process::exit(2);
        }
        [] if cfg!(debug_assertions) => Profile {
            name: "test-smoke",
            corpus_sizes: &[10],
            full_build_samples: 1,
            warm_search_samples: 5,
            incremental_samples: 1,
            enforce_thresholds: false,
        },
        [] => Profile {
            name: "reference",
            corpus_sizes: DEFAULT_CORPUS_SIZES,
            full_build_samples: DEFAULT_FULL_BUILD_SAMPLES,
            warm_search_samples: DEFAULT_WARM_SEARCH_SAMPLES,
            incremental_samples: DEFAULT_INCREMENTAL_SAMPLES,
            enforce_thresholds: true,
        },
    };

    match run(profile) {
        Ok(report) => {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
            if !report.passed {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("docs retrieval benchmark failed: {error}");
            std::process::exit(1);
        }
    }
}

fn run(profile: Profile) -> Result<BenchmarkReport, Box<dyn std::error::Error>> {
    let thresholds = Thresholds {
        reference_documents: 1_000,
        warm_search_p95_ms: 100.0,
        full_build_p95_ms: 10_000.0,
        incremental_refresh_p95_ms: 2_000.0,
        max_cache_source_ratio: 3.0,
    };
    let mut scenarios = Vec::new();
    for &documents in profile.corpus_sizes {
        scenarios.push(run_scenario(documents, profile, &thresholds)?);
    }
    let passed = scenarios.iter().all(|scenario| scenario.passed);
    Ok(BenchmarkReport {
        schema_version: 1,
        benchmark: "slice5_docs_retrieval",
        profile: profile.name.to_string(),
        platform: Platform {
            os: env::consts::OS.to_string(),
            arch: env::consts::ARCH.to_string(),
            rustc: rustc_version(),
        },
        thresholds,
        scenarios,
        passed,
    })
}

fn run_scenario(
    documents: usize,
    profile: Profile,
    thresholds: &Thresholds,
) -> Result<ScenarioReport, Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    setup_repo(temp.path(), documents)?;
    let source_bytes = docs_source_bytes(temp.path())?;

    let mut full_build_times = Vec::new();
    for _ in 0..profile.full_build_samples {
        let cache = temp.path().join(".pulse/cache/docs-search");
        if cache.exists() {
            fs::remove_dir_all(&cache)?;
        }
        let started = Instant::now();
        let report = build_index(
            temp.path(),
            IndexOptions {
                rebuild: true,
                ..IndexOptions::default()
            },
        )?;
        full_build_times.push(started.elapsed());
        assert_eq!(report.documents.indexed as usize, documents);
    }

    let generation = current_generation(temp.path())?
        .ok_or("current docs-search generation missing after benchmark build")?;
    let mut warm_search_times = Vec::new();
    for sample in 0..profile.warm_search_samples {
        let query = format!("TokenExpired shard {} v2.1", sample % documents);
        let terms = tokenize_query_text(&query);
        let started = Instant::now();
        let hits = query_lexical_index(&generation.tantivy_path, &terms, 8)?;
        warm_search_times.push(started.elapsed());
        if hits.is_empty() {
            return Err(format!("warm query returned no result for {documents} documents").into());
        }
    }

    let mut incremental_times = Vec::new();
    let mut changed_documents = Vec::new();
    for sample in 0..profile.incremental_samples {
        let doc_index = sample % documents;
        let path = temp.path().join(doc_path(doc_index));
        let mut bytes = fs::read(&path)?;
        bytes.extend_from_slice(format!("\nIncremental marker {sample}.\n").as_bytes());
        fs::write(path, bytes)?;
        let started = Instant::now();
        let report = build_index(temp.path(), IndexOptions::default())?;
        incremental_times.push(started.elapsed());
        changed_documents.push(report.documents.changed);
    }

    let cache_bytes = directory_bytes(&temp.path().join(".pulse/cache/docs-search"))?;
    let cache_source_ratio = cache_bytes as f64 / source_bytes.max(1) as f64;
    let full_build = summarize(full_build_times);
    let warm_search = summarize(warm_search_times);
    let incremental_refresh = summarize(incremental_times);
    let mut failures = Vec::new();

    if changed_documents.iter().any(|changed| *changed != 1) {
        failures.push(format!(
            "incremental refresh changed counts were {changed_documents:?}, expected all 1"
        ));
    }
    if profile.enforce_thresholds && documents == thresholds.reference_documents {
        if warm_search.p95_ms > thresholds.warm_search_p95_ms {
            failures.push(format!(
                "warm search p95 {:.3} ms exceeded {:.3} ms",
                warm_search.p95_ms, thresholds.warm_search_p95_ms
            ));
        }
        if full_build.p95_ms > thresholds.full_build_p95_ms {
            failures.push(format!(
                "full build p95 {:.3} ms exceeded {:.3} ms",
                full_build.p95_ms, thresholds.full_build_p95_ms
            ));
        }
        if incremental_refresh.p95_ms > thresholds.incremental_refresh_p95_ms {
            failures.push(format!(
                "incremental refresh p95 {:.3} ms exceeded {:.3} ms",
                incremental_refresh.p95_ms, thresholds.incremental_refresh_p95_ms
            ));
        }
        if cache_source_ratio > thresholds.max_cache_source_ratio {
            failures.push(format!(
                "cache/source ratio {:.3} exceeded {:.3}",
                cache_source_ratio, thresholds.max_cache_source_ratio
            ));
        }
    }

    Ok(ScenarioReport {
        documents,
        source_bytes,
        cache_bytes,
        cache_source_ratio,
        full_build,
        warm_search,
        incremental_refresh,
        changed_documents,
        passed: failures.is_empty(),
        failures,
    })
}

fn setup_repo(repo: &Path, documents: usize) -> Result<(), Box<dyn std::error::Error>> {
    let evidence = pulse::evidence::manifest::bootstrap(repo)?.manifest;
    fs::create_dir_all(repo.join(".pulse/docs/schemas"))?;
    let schema: serde_json::Value = serde_json::from_str(DOCUMENT_SCHEMA)?;
    fs::write(
        repo.join(".pulse/docs/schemas/document.schema.json"),
        to_canonical_bytes(&schema)?,
    )?;
    fs::create_dir_all(repo.join("docs/reference"))?;

    let mut records = Vec::with_capacity(documents);
    for index in 0..documents {
        let path = doc_path(index);
        fs::write(repo.join(&path), doc_body(index))?;
        records.push(DocumentRecord {
            id: format!("DOC-BENCH-{index:04}"),
            revision: 1,
            path: path.to_string_lossy().replace('\\', "/"),
            kind: DocumentKind::Reference,
            authority: DocumentAuthority::Approved,
            lifecycle: DocumentLifecycle::Current,
            owner: "system:benchmark".to_string(),
            summary: format!("Reference behavior for benchmark shard {index}"),
            aliases: vec![format!("ShardNeedle{index}"), "refresh-token".to_string()],
            scope: DocumentScope {
                paths: vec![format!("src/shards/{index}/**")],
                domains: vec!["benchmark".to_string()],
                work_labels: vec!["retrieval".to_string()],
            },
            review_policy: ReviewPolicy::None,
            verification_profile: "benchmark".to_string(),
            generated: None,
            superseded_by: None,
            retrieval: None,
        });
    }
    let registry = DocsRegistry {
        schema_version: 2,
        revision: 1,
        repository_id: evidence.repository_id,
        documents: records,
        retrieval: Some(RetrievalConfig {
            materialize_root_index: false,
            area_index_threshold: 1_000,
            ..RetrievalConfig::defaults()
        }),
    };
    fs::write(
        repo.join(".pulse/docs/registry.json"),
        to_canonical_bytes(&registry)?,
    )?;
    Ok(())
}

fn doc_path(index: usize) -> PathBuf {
    PathBuf::from(format!("docs/reference/shard-{index:04}.md"))
}

fn doc_body(index: usize) -> Vec<u8> {
    let mut body = format!(
        "# Benchmark Shard {index}\n\n\
         TokenExpired for shard {index} means the refresh-token expired in v2.1. \
         ShardNeedle{index} maps the request to a deterministic reference section. \
         Rebuild the lexical cache with pulse docs index after changing shard {index}.\n\n"
    );
    for paragraph in 0..24 {
        body.push_str(&format!(
            "Shard {index} paragraph {paragraph} documents deterministic retrieval, cache recovery, \
             source-bound section identity, bounded snippets, registry authority, and offline lexical \
             search behavior. The wording is representative prose rather than repeated padding tokens.\n\n"
        ));
    }
    body.into_bytes()
}

fn docs_source_bytes(repo: &Path) -> Result<u64, Box<dyn std::error::Error>> {
    directory_bytes(&repo.join("docs"))
}

fn directory_bytes(path: &Path) -> Result<u64, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                total += metadata.len();
            }
        }
    }
    Ok(total)
}

fn summarize(mut samples: Vec<Duration>) -> TimingSummary {
    samples.sort_unstable();
    let values = samples
        .iter()
        .map(|duration| duration.as_secs_f64() * 1_000.0)
        .collect::<Vec<_>>();
    let p95_index = ((values.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(values.len().saturating_sub(1));
    TimingSummary {
        samples: values.len(),
        min_ms: values.first().copied().unwrap_or(0.0),
        median_ms: values[values.len() / 2],
        p95_ms: values[p95_index],
        max_ms: values.last().copied().unwrap_or(0.0),
    }
}

fn rustc_version() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
