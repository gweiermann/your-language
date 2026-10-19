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
    scheduled_skips: usize,
    index_bytes: usize,
    observation_bytes: usize,
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
    run_with_partial_policy(language, source, enabled, budget, true)
}

fn run_with_partial_policy(
    language: &CompiledLanguage,
    source: &str,
    enabled: bool,
    budget: Option<usize>,
    partial: bool,
) -> Study {
    let (parsed, occurrences) = crate::runtime::parse_occurrences(language, "study", source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let ranks = super::super::scheduler::query_group_ranks(language, &occurrences).unwrap();
    let image = &language.image.as_ref().unwrap().queries;
    let started = Instant::now();
    let mut engine = Engine::new_with_partial_policy(
        language,
        image,
        &occurrences,
        source,
        &ranks,
        enabled,
        partial,
    );
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
        scheduled_skips: engine.scheduled_skips.get(),
        index_bytes: engine
            .structural_templates
            .as_ref()
            .map_or(0, |index| index.bytes)
            + engine
                .candidate_index
                .as_ref()
                .and_then(|index| index.structural.as_ref())
                .map_or(0, |index| index.bytes),
        observation_bytes: engine.observed_bytes(),
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
            let exact = run_with_partial_policy(&language, &source, true, None, false);
            assert_eq!(baseline.result, exact.result, "exact {}", file.display());
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

#[test]
fn partial_holes_refine_without_waiting_for_unrelated_tuple_positions() {
    let language = compile_text(
        r#"
        import { contribute, get, isPresent } from "core/query"
        symbol blocked
        symbol scope
        symbol declaration
        symbol unrelated
        symbol output
        node Program = "x" {
            semantics {
                when isPresent(get((self, blocked))) {
                    contribute((self, blocked), "cycle")
                }
                contribute(((get((self, blocked)), declaration), "alpha"), "blocked")
                contribute(((get((self, scope)), declaration), "beta"), "ready")
                contribute((self, scope), self)
                contribute((self, output), get(((self, declaration), "beta")))
                when !isPresent(get(((self, unrelated), "alpha"))) {
                    contribute("independent", "settled")
                }
            }
        }
        entry Program
    "#,
    );
    let reference = run(&language, "x", false);
    let exact = run_with_partial_policy(&language, "x", true, None, false);
    let partial = run(&language, "x", true);
    assert_eq!(reference.result, exact.result);
    assert_eq!(reference.result, partial.result);
    assert!(partial.result.states.iter().any(|state| matches!(state,
        ProducerState::Contribution(QueryKey::Text(key), SemanticValue::Text(value))
        if key == "independent" && value == "settled")));
    assert!(partial.result.states.iter().any(|state| matches!(state,
        ProducerState::Contribution(_, SemanticValue::Text(value)) if value == "ready")));
    assert!(partial.scheduled_skips > 0);
}

#[test]
fn guarded_new_candidates_and_ordered_contexts_wake_readers() {
    let language = compile_text(
        r#"
        import { contribute, get, getEarlier, facts, isPresent, equal } from "core/query"
        symbol chosen
        symbol key
        symbol result
        symbol slot
        node Program = "x" {
            semantics {
                contribute("first", getEarlier((self, slot)))
                contribute((self, slot), "earlier")
                contribute("second", getEarlier((self, slot)))
                when equal(get((self, chosen)), true) {
                    contribute((get((self, key)), slot), "later")
                }
                contribute((self, chosen), true)
                contribute((self, key), "different")
                contribute((self, result), get(("different", slot)))
            }
        }
        entry Program
    "#,
    );
    let reference = run(&language, "x", false);
    let exact = run_with_partial_policy(&language, "x", true, None, false);
    let partial = run(&language, "x", true);
    assert_eq!(reference.result, exact.result);
    assert_eq!(reference.result, partial.result);
    assert!(partial.result.states.iter().any(|state| matches!(state,
        ProducerState::Contribution(QueryKey::Text(key), SemanticValue::Text(value))
        if key == "second" && value == "earlier")));
}

#[test]
#[ignore = "release-mode partial structural key study"]
fn query_lifetime_study_partial_keys() {
    let opaque = structural::StructuralIndex::new((0..500).map(|id| {
        (
            id,
            Pattern::Tuple(vec![
                Pattern::Any,
                Pattern::Exact(QueryKey::Symbol(format!("family{id}"))),
            ]),
        )
    }));
    let retained = opaque.candidates(&Pattern::Any).len();
    assert_eq!(retained, 500);
    println!("opaque-reader conservative_candidates=500 indexed_candidates={retained} index_bytes={} pruning_gain=0", opaque.bytes);
    let language = compile_text(
        r#"
        import { contribute, get, isPresent, text } from "core/query"
        symbol blocked
        symbol owner
        symbol declaration
        symbol value
        node Item = name: /[a-z]+/ {
            semantics {
                when isPresent(get((self, blocked))) {
                    contribute((self, blocked), "cycle")
                }
                contribute((get((self, blocked)), declaration, text(name)), "unresolved")
                contribute((self, owner), self)
                contribute((get((self, owner)), value, text(name)), "ready")
                contribute((self, value), get((self, value, text(name))))
            }
        }
        node Program = items: Item*
        trivia Whitespace = /\s+/
        entry Program { trivia Whitespace }
    "#,
    );
    for count in [50, 200, 500] {
        let source = (0..count)
            .map(|number| {
                let mut number = number;
                let mut result = "name".to_owned();
                loop {
                    result.push((b'a' + (number % 26) as u8) as char);
                    number /= 26;
                    if number == 0 {
                        break;
                    }
                }
                result
            })
            .collect::<Vec<_>>()
            .join(" ");
        let conservative = run(&language, &source, false);
        let exact = run_with_partial_policy(&language, &source, true, None, false);
        let sparse = run(&language, &source, true);
        assert_eq!(conservative.result, exact.result);
        assert_eq!(conservative.result, sparse.result);
        println!("partial count={count} conservative_us={} exact_us={} sparse_us={} conservative_checks={} exact_checks={} sparse_checks={} sparse_skips={} memo_peak_bytes={} index_bytes={} observation_bytes={}",
            conservative.elapsed.as_micros(), exact.elapsed.as_micros(), sparse.elapsed.as_micros(),
            conservative.candidate_checks, exact.candidate_checks, sparse.candidate_checks,
            sparse.scheduled_skips, sparse.memo.peak_bytes, sparse.index_bytes, sparse.observation_bytes);
    }
}

#[test]
fn wakeup_observation_budget_falls_back_to_evaluation() {
    let language = compile_text(
        r#"
        import { contribute, get } from "core/query"
        node Program = "x" { semantics {
            contribute("answer", get("seed"))
            contribute("seed", "ready")
        } }
        entry Program
    "#,
    );
    let (_, occurrences) = crate::runtime::parse_occurrences(&language, "study", "x");
    let ranks = super::super::scheduler::query_group_ranks(&language, &occurrences).unwrap();
    let image = &language.image.as_ref().unwrap().queries;
    let engine = Engine::new(&language, image, &occurrences, "x", &ranks);
    let reads = BTreeSet::from([QueryKey::Text("seed".into())]);
    assert!(engine.retain_observation(&reads).is_some());
    engine.observation_budget.set(4 * 1024 * 1024);
    assert!(engine.retain_observation(&reads).is_none());
}
