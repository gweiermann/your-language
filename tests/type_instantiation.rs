use std::collections::BTreeMap;
use your_language::{
    diagnostic::Diagnostic,
    semantics::{
        types, AnalysisResult, Evaluation, NativeOperation, OperationContext, OperationParameter,
        OperationSignature, ParameterKind, SemanticEngine, SemanticIdentity, SemanticKey,
        SemanticValue,
    },
};

struct Own;
impl NativeOperation for Own {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let locals = match context.argument("local").unwrap() {
            SemanticValue::Identity(local) => vec![local.clone()],
            SemanticValue::List(values) => values
                .iter()
                .map(|value| {
                    let SemanticValue::Identity(identity) = value else {
                        unreachable!()
                    };
                    identity.clone()
                })
                .collect(),
            _ => unreachable!(),
        };
        types::register_template(
            context,
            SemanticIdentity::Occurrence(context.occurrence.id),
            &locals,
        )
    }
}
struct Remember;
impl NativeOperation for Remember {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let SemanticValue::Text(name) = context.argument("name").unwrap() else {
            unreachable!()
        };
        let value = context.argument("value").unwrap().clone();
        context.attach(&format!("test#{name}"), value);
        Ok(())
    }
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        Ok(Evaluation::Ready(
            context.argument("value").unwrap().clone(),
        ))
    }
}
fn engine() -> SemanticEngine {
    let mut engine = SemanticEngine::default();
    for (name, args, contribution) in [
        ("own", vec!["local"], true),
        ("remember", vec!["name", "value"], false),
    ] {
        engine
            .registry_mut()
            .register(
                "host",
                name,
                OperationSignature {
                    id: format!("host#{name}"),
                    provides_context: false,
                    finalization_only: true,
                    contribution,
                    parameters: args
                        .into_iter()
                        .map(|name| OperationParameter {
                            name: name.into(),
                            kind: if name == "name" {
                                ParameterKind::Text
                            } else {
                                ParameterKind::Value
                            },
                            default: None,
                        })
                        .collect(),
                },
                move || {
                    if name == "own" {
                        Box::new(Own) as Box<dyn NativeOperation>
                    } else {
                        Box::new(Remember)
                    }
                },
            )
            .unwrap();
    }
    engine
}
const HEADER: &str = r#"
import * as types from "std/types"
import * as syntax from "std/syntax"
import * as host from "host"
symbol Alpha
symbol Beta
symbol Shape
symbol input
symbol output
node Local = "p"
node First = "n" { meanings { types::annotate(syntax::identity(), Alpha) } }
node Second = "s" { meanings { types::annotate(syntax::identity(), Beta) } }
node Template = "f" local: Local {
    meanings {
        host::own(syntax::identityOf(local))
        types::annotate(syntax::identity(), Shape)
        types::annotate((syntax::identity(), input), types::typeof(syntax::identityOf(local)))
        types::annotate(syntax::identityOf(local), types::typeof((syntax::identity(), input)))
        types::annotate((syntax::identity(), output), types::typeof(syntax::identityOf(local)))
    }
}
operation instance(template, site) = types::instantiate(syntax::identityOf(template), syntax::identityOf(site))
"#;
fn analyze(extra: &str, actions: &str) -> AnalysisResult {
    analyze_header(HEADER, extra, actions, "fpns")
}
fn analyze_header(header: &str, extra: &str, actions: &str, program: &str) -> AnalysisResult {
    let source = format!("{header}\n{extra}\nnode Root = template: Template first: First second: Second {{ meanings {{ {actions} }} }}\nentry Root");
    let engine = engine();
    let language = engine
        .compile("main.yl", &BTreeMap::from([("main.yl".into(), source)]))
        .unwrap();
    let reloaded = engine.load(&language.to_bytes().unwrap()).unwrap();
    engine.analyze(&reloaded, "program", program)
}
fn attachment(result: &AnalysisResult, name: &str) -> SemanticValue {
    result
        .graph
        .attachments
        .iter()
        .find(|item| item.key == format!("test#{name}"))
        .unwrap()
        .value
        .clone()
}
fn value(result: &AnalysisResult, identity: SemanticValue, slot: &str) -> SemanticValue {
    let key = SemanticValue::Tuple(vec![
        identity,
        SemanticValue::Symbol(format!("main.yl#{slot}")),
    ]);
    let row = result
        .graph
        .records
        .iter()
        .find(|row| {
            row.entries
                .iter()
                .any(|(k, v)| *k == SemanticKey::Symbol("std/types#key".into()) && *v == key)
        })
        .unwrap();
    row.entries
        .iter()
        .find(|(k, _)| *k == SemanticKey::Symbol("std/types#type".into()))
        .unwrap()
        .1
        .clone()
}
const CALLS: &str = r#"
host::remember("a", instance(template, first))
host::remember("a-again", instance(template, first))
host::remember("b", instance(template, second))
types::annotate((instance(template, first), input), types::typeof(syntax::identityOf(first)))
types::annotate((instance(template, second), input), types::typeof(syntax::identityOf(second)))
"#;

#[test]
fn independent_instances_retain_one_shared_local_relationship() {
    let result = analyze("", CALLS);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let a = attachment(&result, "a");
    let b = attachment(&result, "b");
    assert_eq!(a, attachment(&result, "a-again"));
    assert_ne!(a, b);
    assert_eq!(
        value(&result, a, "output"),
        SemanticValue::Symbol("main.yl#Alpha".into())
    );
    assert_eq!(
        value(&result, b, "output"),
        SemanticValue::Symbol("main.yl#Beta".into())
    );
    let root = &result.occurrences[0];
    let template = SemanticValue::Identity(SemanticIdentity::Occurrence(
        root.captures["template"].occurrences[0],
    ));
    assert_eq!(
        value(&result, template, "input"),
        SemanticValue::Symbol("std/types#unknown".into())
    );
}

#[test]
fn captured_external_type_is_not_freshened() {
    let actions = format!(
        r#"
        types::annotate((syntax::identityOf(template), output), types::typeof(syntax::identityOf(first)))
        {CALLS}
    "#
    );
    let header = HEADER.replace(
        "types::annotate((syntax::identity(), output), types::typeof(syntax::identityOf(local)))",
        "",
    );
    let result = analyze_header(&header, "", &actions, "fpns");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let output = value(&result, attachment(&result, "b"), "output");
    assert_eq!(output, SemanticValue::Symbol("main.yl#Alpha".into()));
}

#[test]
fn restrictions_preserve_call_argument_and_bound_origins() {
    let actions = format!(
        r#"
        types::restrictTo((syntax::identityOf(template), input), Alpha)
        {CALLS}
    "#
    );
    let result = analyze("", &actions);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].code, "types.mismatch");
    assert_eq!(result.diagnostics[0].primary.start, 3);
    assert_eq!(result.diagnostics[0].primary.end, 4);
    assert_eq!(result.diagnostics[0].secondary.len(), 1);
}

#[test]
fn unregistered_template_reports_ownership_instead_of_guessing() {
    let result = analyze(
        "",
        r#"host::remember("bad", types::instantiate(syntax::identityOf(first), syntax::identityOf(second)))"#,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "types.template_ownership"),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn parallel_analyses_have_disjoint_analysis_local_state() {
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(independent_instances_retain_one_shared_local_relationship);
        }
    });
}

#[test]
fn alias_retains_template_origin_without_sharing_instance_variables() {
    let result = analyze(
        "",
        r#"
        types::annotate(syntax::identityOf(first), types::typeof(syntax::identityOf(template)))
        host::remember("alias", types::instantiate(syntax::identityOf(first), syntax::identityOf(second)))
        types::annotate((types::instantiate(syntax::identityOf(first), syntax::identityOf(second)), input), Beta)
    "#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        value(&result, attachment(&result, "alias"), "output"),
        SemanticValue::Symbol("main.yl#Beta".into())
    );
}

#[test]
fn independent_list_positions_are_freshened_consistently() {
    let header = HEADER
        .replace("local: Local", "local: Local other: Local")
        .replace(
            "host::own(syntax::identityOf(local))",
            "host::own([syntax::identityOf(local), syntax::identityOf(other)])",
        )
        .replace(
            "types::typeof(syntax::identityOf(local))",
            "[types::typeof(syntax::identityOf(local)), types::typeof(syntax::identityOf(other))]",
        )
        .replace(
            "types::annotate(syntax::identityOf(local),",
            "types::annotate([syntax::identityOf(local), syntax::identityOf(other)],",
        );
    let actions = CALLS
        .replace(
            "types::typeof(syntax::identityOf(first)))",
            "[Alpha, Beta])",
        )
        .replace(
            "types::typeof(syntax::identityOf(second)))",
            "[Beta, Alpha])",
        );
    let result = analyze_header(&header, "", &actions, "fppns");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        value(&result, attachment(&result, "a"), "output"),
        SemanticValue::List(vec![
            SemanticValue::Symbol("main.yl#Alpha".into()),
            SemanticValue::Symbol("main.yl#Beta".into())
        ])
    );
    assert_eq!(
        value(&result, attachment(&result, "b"), "output"),
        SemanticValue::List(vec![
            SemanticValue::Symbol("main.yl#Beta".into()),
            SemanticValue::Symbol("main.yl#Alpha".into())
        ])
    );
}

#[test]
fn templates_with_residual_matches_are_explicitly_parked() {
    let header = HEADER.replace(
        "host::own(syntax::identityOf(local))",
        r#"
        host::own(syntax::identityOf(local))
        match (types::typeof(syntax::identityOf(local))) {
            _ => types::annotate(syntax::identity(), Alpha)
        }
    "#,
    );
    let result = analyze_header(&header, "", "", "fpns");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "types.template_match"),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn ownership_cannot_change_after_registration() {
    let header = HEADER.replace(
        "host::own(syntax::identityOf(local))",
        "host::own(syntax::identityOf(local))\nhost::own(syntax::identity())",
    );
    let result = analyze_header(&header, "", "", "fpns");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "types.template_ownership"),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn finite_recursive_relationships_propagate_within_each_instance() {
    let header = HEADER.replace("host::own(syntax::identityOf(local))", "host::own(syntax::identityOf(local))\ntypes::annotate(syntax::identityOf(local), types::typeof(syntax::identityOf(local)))");
    let result = analyze_header(&header, "", CALLS, "fpns");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        value(&result, attachment(&result, "a"), "output"),
        SemanticValue::Symbol("main.yl#Alpha".into())
    );
    assert_eq!(
        value(&result, attachment(&result, "b"), "output"),
        SemanticValue::Symbol("main.yl#Beta".into())
    );
}

#[test]
fn mixed_local_and_captured_destinations_are_explicitly_parked() {
    let result = analyze(
        "",
        &format!(
            r#"
        types::annotate([syntax::identityOf(template), syntax::identityOf(first)], [Shape, Alpha])
        {CALLS}
    "#
        ),
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "types.template_ownership"
                && diagnostic.message.contains("positional")),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn public_host_example_preserves_human_and_json_underlines() {
    use std::process::Command;
    let built = Command::new(env!("CARGO"))
        .args(["build", "--quiet", "--example", "type_instantiation"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_yl"))
        .parent()
        .unwrap()
        .join("examples")
        .join(format!(
            "type_instantiation{}",
            std::env::consts::EXE_SUFFIX
        ));
    let success = Command::new(&executable).output().unwrap();
    assert!(success.status.success());
    let output = String::from_utf8(success.stdout).unwrap();
    assert!(output.contains("first: {\"Symbol\":\"main.yl#number\"}"));
    assert!(output.contains("second: {\"Symbol\":\"main.yl#string\"}"));
    let human = Command::new(&executable)
        .arg("errors")
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .output()
        .unwrap();
    assert_eq!(human.status.code(), Some(1));
    let output = String::from_utf8(human.stderr).unwrap();
    assert!(output.contains("\x1b[31m"));
    assert!(output.contains("^^^^^^^"));
    let json = Command::new(&executable)
        .args(["errors", "--json"])
        .output()
        .unwrap();
    assert_eq!(json.status.code(), Some(1));
    assert!(json.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let source = include_str!("../examples/type_instantiation/programs/errors.txt");
    let diagnostic = &json["diagnostics"][0];
    let primary = &diagnostic["primary"];
    assert_eq!(
        &source[primary["start"].as_u64().unwrap() as usize
            ..primary["end"].as_u64().unwrap() as usize],
        "\"wrong\""
    );
    let related = &diagnostic["secondary"][0];
    assert_eq!(
        &source[related["start"].as_u64().unwrap() as usize
            ..related["end"].as_u64().unwrap() as usize],
        "number"
    );
}

#[test]
fn late_alias_ownership_cannot_switch_a_cached_instance_template() {
    let result = analyze(
        "",
        r#"
        types::annotate(syntax::identity(), types::typeof(syntax::identityOf(template)))
        host::remember("a", types::instantiate(syntax::identity(), syntax::identityOf(first)))
        host::own(syntax::identityOf(first))
    "#,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "types.template_ownership"),
        "{:?}",
        result.diagnostics
    );
}
