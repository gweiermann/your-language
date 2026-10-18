//! Equivalence and measurable lifetime studies over real compiled examples.
use super::*;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq)]
struct Results {
    states: Vec<ProducerState>,
    reads: Vec<BTreeSet<QueryKey>>,
    waits: Vec<BTreeSet<usize>>,
    diagnostics: Vec<Diagnostic>,
    producer_diagnostics: Vec<Vec<Diagnostic>>,
}
struct Study {
    result: Results,
    elapsed: Duration,
    memo: memo::Stats,
    hypothetical: memo::Stats,
    candidate_checks: usize,
    exact_writers: usize,
    residual_writers: usize,
    store_free_functions: usize,
    functions: usize,
    static_producers: usize,
    producers: usize,
    retired: usize,
}

fn run(language: &CompiledLanguage, source: &str, enabled: bool) -> Study {
    run_with_budget(language, source, enabled, None)
}

fn run_with_budget(
    language: &CompiledLanguage,
    source: &str,
    enabled: bool,
    budget: Option<usize>,
) -> Study {
    let (parsed, occurrences) = crate::runtime::parse_occurrences(language, "study", source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let ranks = super::super::scheduler::query_group_ranks(language, &occurrences).unwrap();
    let image = &language.image.as_ref().unwrap().queries;
    let started = Instant::now();
    let mut engine =
        Engine::new_with_policy(language, image, &occurrences, source, &ranks, enabled);
    if let Some(budget) = budget {
        engine.function_cache = RefCell::new(memo::Memo::new(budget));
    }
    let diagnostics = engine.settle();
    let elapsed = started.elapsed();
    let index = CandidateIndex::new(&engine.producers);
    let memo = engine.function_cache.borrow().stats.clone();
    let hypothetical = engine.hypothetical_cache.borrow().stats.clone();
    Study {
        result: Results {
            states: engine.producers.iter().map(|p| p.state.clone()).collect(),
            reads: engine.producers.iter().map(|p| p.reads.clone()).collect(),
            waits: engine.producers.iter().map(|p| p.waits.clone()).collect(),
            producer_diagnostics: engine
                .producers
                .iter()
                .map(|p| p.diagnostics.clone())
                .collect(),
            diagnostics,
        },
        elapsed,
        memo,
        hypothetical,
        candidate_checks: engine.lookup_candidates.get(),
        exact_writers: index.exact.values().map(Vec::len).sum(),
        residual_writers: index.residual.len(),
        store_free_functions: engine
            .lifetime_plan
            .store_dependent_functions
            .values()
            .filter(|dependent| !**dependent)
            .count(),
        functions: engine.lifetime_plan.store_dependent_functions.len(),
        static_producers: engine.store_free_producers.len(),
        producers: engine.producers.len(),
        retired: engine.retired_evaluations.len(),
    }
}

fn language(example: &str) -> CompiledLanguage {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let project = crate::project::Project::load(&root.join("examples").join(example)).unwrap();
    let compiled = crate::compile_language(project.entry).unwrap();
    crate::load_compiled_language(&compiled.to_bytes().unwrap()).unwrap()
}

#[test]
fn caches_and_collection_preserve_real_query_results_and_dependencies() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for example in [
        "07-declarative-queries",
        "09-type-instantiation",
        "10-declarative-functions",
        "11-compound-types",
    ] {
        let language = language(example);
        for file in std::fs::read_dir(root.join("examples").join(example).join("programs")).unwrap()
        {
            let file = file.unwrap().path();
            if file.extension().is_none_or(|extension| extension != "txt") {
                continue;
            }
            let source = std::fs::read_to_string(&file).unwrap();
            let baseline = run(&language, &source, false);
            let collected = run(&language, &source, true);
            assert_eq!(baseline.result, collected.result, "{}", file.display());
            assert!(collected.memo.peak_bytes <= 4 * 1024 * 1024);
            assert!(collected.hypothetical.peak_bytes <= 4 * 1024 * 1024);
        }
    }
}

#[test]
fn lifetime_plan_is_validated_and_legacy_images_derive_the_same_plan() {
    let language = language("10-declarative-functions");
    let mut json: serde_json::Value =
        serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    let plan = json["queries"]
        .as_object_mut()
        .unwrap()
        .remove("lifetimes")
        .unwrap();
    let old = crate::load_compiled_language(&serde_json::to_vec(&json).unwrap()).unwrap();
    let source = include_str!("../../../examples/10-declarative-functions/programs/identity.txt");
    assert_eq!(
        run(&language, source, true).result,
        run(&old, source, true).result
    );
    json["queries"]["lifetimes"] = plan;
    json["queries"]["lifetimes"]["reads"] = serde_json::json!([]);
    assert!(crate::load_compiled_language(&serde_json::to_vec(&json).unwrap()).is_err());
}

/// Reproducible release study: run explicitly, outside normal test timing noise.
#[test]
#[ignore = "release-mode performance study"]
fn query_lifetime_study() {
    for (example, program) in [
        ("07-declarative-queries", "shadowing.txt"),
        ("09-type-instantiation", "program.txt"),
        ("10-declarative-functions", "generic-addition.txt"),
        ("11-compound-types", "valid.txt"),
        ("11-compound-types", "invalid-assignment.txt"),
    ] {
        let language = language(example);
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join(example)
                .join("programs")
                .join(program),
        )
        .unwrap();
        for enabled in [false, true] {
            let mut times = vec![];
            let mut last = run(&language, &source, enabled);
            for _ in 0..7 {
                last = run(&language, &source, enabled);
                times.push(last.elapsed.as_micros());
            }
            times.sort_unstable();
            println!("{{\"example\":\"{example}/{program}\",\"enabled\":{enabled},\"median_us\":{},\"candidate_checks\":{},\"exact_writers\":{},\"residual_writers\":{},\"store_free_functions\":{},\"functions\":{},\"static_producers\":{},\"producers\":{},\"retired\":{},\"memo_hits\":{},\"memo_misses\":{},\"memo_collected\":{},\"memo_peak_bytes\":{},\"hypothetical_hits\":{},\"hypothetical_peak_bytes\":{}}}",
                times[3], last.candidate_checks, last.exact_writers, last.residual_writers,
                last.store_free_functions, last.functions, last.static_producers, last.producers,
                last.retired, last.memo.hits, last.memo.misses,
                last.memo.collected, last.memo.peak_bytes, last.hypothetical.hits,
                last.hypothetical.peak_bytes);
        }
    }
}

fn compile_text(source: &str) -> CompiledLanguage {
    let language = SemanticEngine::default()
        .compile(
            "study.yl",
            &BTreeMap::from([("study.yl".into(), source.into())]),
        )
        .unwrap();
    crate::load_compiled_language(&language.to_bytes().unwrap()).unwrap()
}

#[test]
fn dynamic_guards_fact_cycles_and_negative_reads_survive_collection() {
    for actions in [
        r#"contributeAll((self, left), facts((self, right)))
            contributeAll((self, right), facts((self, left)))
            contribute((self, left), "seed")
            contribute((self, result), getAll((self, right)))"#,
        r#"when isPresent(get((self, left))) {
            contribute((self, left), "cycle")
        }
        contribute((self, right), "ready")"#,
        r#"when equal(get((self, left)), true) {
            contribute((self, right), "yes")
        } else {
            contribute((self, right), "no")
        }
        contribute((self, left), true)
        contribute((get((self, right)), result), "dynamic")"#,
    ] {
        let source = format!(
            r#"
        import {{ contribute, contributeAll, facts, getAll, get, isPresent, equal }} from "core/query"
        symbol left
        symbol right
        symbol result
        node Program = "x" {{ semantics {{ {actions} }} }}
        entry Program
        "#
        );
        let language = compile_text(&source);
        assert_eq!(
            run(&language, "x", false).result,
            run(&language, "x", true).result
        );
    }
}

#[test]
#[ignore = "release-mode scale study"]
fn query_lifetime_study_scale() {
    let language = compile_text(
        r#"
        import { contribute, get, text, equal } from "core/query"
        import { error } from "core/diagnostics"
        symbol slot
        operation read(name) = get((name, slot))
        node Item = name: /[a-z]+/ {
            semantics {
                contribute((text(name), slot), text(name))
                when !equal(read(text(name)), text(name)) {
                    error("Unexpected value")
                }
            }
        }
        node Program = items: Item*
        trivia Whitespace = /\s+/
        entry Program { trivia Whitespace }
    "#,
    );
    for count in [100, 400, 1000] {
        let source = (0..count)
            .map(|index| {
                let mut number = index;
                let mut name = "item".to_owned();
                loop {
                    name.push((b'a' + (number % 26) as u8) as char);
                    number /= 26;
                    if number == 0 {
                        break;
                    }
                }
                name
            })
            .collect::<Vec<_>>()
            .join(" ");
        let baseline = run(&language, &source, false);
        let collected = run(&language, &source, true);
        assert_eq!(baseline.result, collected.result);
        println!("scale count={count} baseline_us={} optimized_us={} baseline_checks={} optimized_checks={} static_producers={}/{} retired={} peak_memo_bytes={} collected_memos={}",
            baseline.elapsed.as_micros(), collected.elapsed.as_micros(), baseline.candidate_checks,
            collected.candidate_checks, collected.static_producers, collected.producers,
            collected.retired, collected.memo.peak_bytes, collected.memo.collected);
    }
}

#[test]
#[ignore = "release-mode retention study"]
fn query_lifetime_study_retention() {
    let language = compile_text(
        r#"
        import { contribute, text, isPresent, tail } from "core/query"
        symbol slot
        operation result(parts) = match (isPresent(parts[0])) {
            true => result(tail(parts)),
            _ => "ok"
        }
        node Item = name: /[a-z]+/ {
            semantics { contribute((text(name), slot), result([text(name)])) }
        }
        node Program = items: Item*
        trivia Whitespace = /\s+/
        entry Program { trivia Whitespace }
    "#,
    );
    let source = (0..6000)
        .map(|index| {
            let mut number = index;
            let mut name = "longname".repeat(8);
            loop {
                name.push((b'a' + (number % 26) as u8) as char);
                number /= 26;
                if number == 0 {
                    break;
                }
            }
            name
        })
        .collect::<Vec<_>>()
        .join(" ");
    let bounded = run(&language, &source, true);
    let unbounded = run_with_budget(&language, &source, true, Some(usize::MAX));
    assert_eq!(bounded.result, unbounded.result);
    assert!(bounded.memo.peak_bytes <= 4 * 1024 * 1024);
    assert!(bounded.memo.evictions > 0);
    assert!(unbounded.memo.peak_bytes > bounded.memo.peak_bytes);
    println!("retention count=6000 bounded_us={} unbounded_us={} bounded_peak_bytes={} unbounded_peak_bytes={} evictions={} static_producers={}/{}",
        bounded.elapsed.as_micros(), unbounded.elapsed.as_micros(), bounded.memo.peak_bytes,
        unbounded.memo.peak_bytes, bounded.memo.evictions, bounded.static_producers, bounded.producers);
}
