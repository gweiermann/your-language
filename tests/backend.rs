use std::{
    collections::BTreeMap,
    sync::{atomic::AtomicBool, Arc},
};
use your_language::{
    backend::{
        Backend, BackendContext, Capture, DispatchOptions, HookBinding, HookContract, SyntaxRef,
    },
    compile_sources,
    diagnostic::{Diagnostic, Span},
    load_compiled_language,
};
fn language(source: &str) -> your_language::CompiledLanguage {
    let result = compile_sources(
        "main.yl",
        &BTreeMap::from([("main.yl".into(), source.into())]),
    );
    assert!(result.is_ok(), "{result:?}");
    load_compiled_language(&result.unwrap().to_bytes().unwrap()).unwrap()
}
#[derive(Default)]
struct Host {
    calls: usize,
    bindings: Option<Vec<HookBinding>>,
    seen: Vec<SyntaxRef>,
}
impl Backend for Host {
    type Value = String;
    fn bindings(&self) -> Vec<HookBinding> {
        self.bindings.clone().unwrap_or_else(|| {
            vec![
                HookBinding::new("main.yl#literal", &["value"]),
                HookBinding::new("main.yl#pair", &["left", "right"]),
            ]
        })
    }
    fn invoke(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        hook: &HookContract,
        args: &[Capture],
    ) -> Result<String, Diagnostic> {
        self.calls += 1;
        match hook.id.as_str() {
            "main.yl#literal" => {
                let value = args[0].syntax()?;
                self.seen.push(value);
                Ok(context.program().text(value)?.into())
            }
            "main.yl#pair" => {
                let left = context.dispatch(self, args[0].syntax()?)?;
                let right = context.dispatch(self, args[1].syntax()?)?;
                Ok(format!("{left}:{right}"))
            }
            _ => Err(Diagnostic::error(
                "host.unknown",
                "Unknown host hook",
                Span::default(),
            )),
        }
    }
}
const SIMPLE:&str="hook literal(value) hook pair(left,right) node Value = value: /[0-9]+/ { backend { literal(value) } } node Program = left: Value \",\" right: Value { backend { pair(left,right) } } entry Program";
#[test]
fn dispatch_sourcefree_without_eager_children() {
    let lang = language(SIMPLE);
    let program = lang.prepare_backend("input", "12,34").unwrap();
    let mut host = Host::default();
    assert_eq!(host.calls, 0);
    assert_eq!(program.run(&mut host).unwrap(), "12:34");
    assert_eq!(host.calls, 3);
    assert_eq!(program.text(host.seen[0]).unwrap(), "12");
    assert_eq!(
        program.span(host.seen[0]).unwrap(),
        &Span::new("input", 0, 2)
    );
    assert_eq!(program.run(&mut host).unwrap(), "12:34");
    assert_eq!(host.calls, 6);
}
#[test]
fn entire_manifest_checked_before_any_effects() {
    let lang = language(SIMPLE);
    let program = lang.prepare_backend("input", "1,2").unwrap();
    for bindings in [
        vec![],
        vec![HookBinding::new("main.yl#literal", &["wrong"])],
        vec![
            HookBinding::new("main.yl#literal", &["value"]),
            HookBinding::new("main.yl#literal", &["value"]),
        ],
    ] {
        let mut host = Host {
            bindings: Some(bindings),
            ..Host::default()
        };
        assert_eq!(
            program.run(&mut host).unwrap_err().code,
            "yl.backend_binding"
        );
        assert_eq!(host.calls, 0);
    }
}
#[test]
fn cancelled_and_bounded_before_effects() {
    let lang = language(SIMPLE);
    let program = lang.prepare_backend("input", "1,2").unwrap();
    let mut host = Host::default();
    let options = DispatchOptions {
        max_calls: 0,
        ..DispatchOptions::default()
    };
    assert_eq!(
        program
            .run_with_options(&mut host, options)
            .unwrap_err()
            .code,
        "yl.backend_limit"
    );
    assert_eq!(host.calls, 0);
    let options = DispatchOptions {
        cancelled: Some(Arc::new(AtomicBool::new(true))),
        ..DispatchOptions::default()
    };
    assert_eq!(
        program
            .run_with_options(&mut host, options)
            .unwrap_err()
            .code,
        "yl.backend_cancelled"
    );
    assert_eq!(host.calls, 0);
}
#[test]
fn syntax_handles_are_program_scoped() {
    let lang = language(SIMPLE);
    let first = lang.prepare_backend("one", "1,2").unwrap();
    let second = lang.prepare_backend("two", "3,4").unwrap();
    assert_eq!(
        second.text(first.root()).unwrap_err().code,
        "yl.backend_handle"
    );
}
#[test]
fn bad_hooks_are_compile_errors() {
    for (source, code) in [
        (
            "hook x(a) node Program = \":\" { backend { x() } } entry Program",
            "yl.backend_arity",
        ),
        (
            "node Program = \":\" { backend { x() } } entry Program",
            "yl.unknown_reference",
        ),
        (
            "hook x(a) node Program = \":\" { backend { x(missing) } } entry Program",
            "yl.artifact",
        ),
        (
            "hook x(a) node Program = \":\" { backend { x(\"eager\") } } entry Program",
            "yl.backend_argument",
        ),
        (
            "hook x() trivia Space = / +/ { backend { x() } } node Program = \":\" entry Program",
            "yl.backend_scope",
        ),
    ] {
        let result = compile_sources(
            "main.yl",
            &BTreeMap::from([("main.yl".into(), source.into())]),
        );
        assert_eq!(result.unwrap_err()[0].code, code, "{source}");
    }
}
#[test]
fn corrupt_backend_image_is_rejected() {
    let lang = language(SIMPLE);
    let bytes = lang.to_bytes().unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for corrupt in ["id", "arity", "capture", "owner", "untraced"] {
        let mut value = original.clone();
        match corrupt {
            "id" => value["backend"]["calls"]["main.yl#Program"]["hook"] = 999.into(),
            "arity" => value["backend"]["hooks"][0]["parameters"] = serde_json::json!([]),
            "capture" => {
                value["backend"]["calls"]["main.yl#Program"]["arguments"][0] =
                    serde_json::json!({"Capture":"missing"})
            }
            "untraced" => {
                value["semantics"]
                    .as_object_mut()
                    .unwrap()
                    .remove("main.yl#Program");
            }
            _ => {
                let call = value["backend"]["calls"]["main.yl#Program"].clone();
                value["backend"]["calls"]["missing#owner"] = call;
            }
        }
        assert_eq!(
            load_compiled_language(&serde_json::to_vec(&value).unwrap()).unwrap_err()[0].code,
            "yl.artifact"
        );
    }
}
#[test]
fn imports_aliases_patterns_repetition_and_absence_keep_capture_shapes() {
    let sources=BTreeMap::from([
        ("main.yl".into(),r#"import * as runtime from "./contracts" import { literal as number } from "./contracts" import { separatedBy } from "std/parser"
        node Value = value:/[0-9]+/ { backend { number(value) } }
        pattern Items = items:Value* |> separatedBy(",") { backend { runtime::collect(items) } }
        node Program = values:Items optional:Value? { backend { runtime::inspect(self, values, optional, (values, optional), [values]) } } entry Program"#.into()),
        ("contracts.yl".into(),"export hook literal(value) export hook collect(items) export hook inspect(owner,values,optional,pair,list)".into())]);
    let compiled = compile_sources("main.yl", &sources).unwrap();
    let language = load_compiled_language(&compiled.to_bytes().unwrap()).unwrap();
    struct Shapes;
    impl Backend for Shapes {
        type Value = String;
        fn bindings(&self) -> Vec<HookBinding> {
            vec![
                HookBinding::new("contracts.yl#literal", &["value"]),
                HookBinding::new("contracts.yl#collect", &["items"]),
                HookBinding::new(
                    "contracts.yl#inspect",
                    &["owner", "values", "optional", "pair", "list"],
                ),
            ]
        }
        fn invoke(
            &mut self,
            cx: &mut BackendContext<'_, '_, Self>,
            hook: &HookContract,
            args: &[Capture],
        ) -> Result<String, Diagnostic> {
            match hook.id.as_str() {
                "contracts.yl#literal" => Ok(cx.program().text(args[0].syntax()?)?.into()),
                "contracts.yl#collect" => {
                    let mut values = vec![];
                    for item in args[0].list()? {
                        values.push(cx.dispatch(self, item.syntax()?)?);
                    }
                    Ok(values.join(":"))
                }
                _ => {
                    assert!(matches!(args[2], Capture::Absent));
                    assert!(matches!(args[3], Capture::Tuple(_)));
                    assert!(matches!(args[4], Capture::List(_)));
                    let owner = args[0].syntax()?;
                    assert_eq!(cx.program().capture(owner, "values")?, args[1]);
                    cx.dispatch(self, args[1].syntax()?)
                }
            }
        }
    }
    let program = language.prepare_backend("list", "1,2,3").unwrap();
    assert_eq!(program.run(&mut Shapes).unwrap(), "1:2:3");
}
#[test]
fn self_recursion_is_bounded_and_host_errors_keep_grouped_sources() {
    let language = language(
        "hook recur(value) node Program = value:/[a-z]+/ { backend { recur(self) } } entry Program",
    );
    let program = language.prepare_backend("input", "hello").unwrap();
    struct Recursive;
    impl Backend for Recursive {
        type Value = ();
        fn bindings(&self) -> Vec<HookBinding> {
            vec![HookBinding::new("main.yl#recur", &["value"])]
        }
        fn invoke(
            &mut self,
            cx: &mut BackendContext<'_, '_, Self>,
            _: &HookContract,
            args: &[Capture],
        ) -> Result<(), Diagnostic> {
            cx.dispatch(self, args[0].syntax()?)
        }
    }
    let error = program
        .run_with_options(
            &mut Recursive,
            DispatchOptions {
                max_calls: 8,
                ..DispatchOptions::default()
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "yl.backend_limit");
    assert_eq!(*error.primary, Span::new("input", 0, 5));
    struct Fails;
    impl Backend for Fails {
        type Value = ();
        fn bindings(&self) -> Vec<HookBinding> {
            vec![HookBinding::new("main.yl#recur", &["value"])]
        }
        fn invoke(
            &mut self,
            _: &mut BackendContext<'_, '_, Self>,
            _: &HookContract,
            _: &[Capture],
        ) -> Result<(), Diagnostic> {
            let mut diagnostic = Diagnostic::error("host.failure", "Host failed", Span::default());
            diagnostic
                .annotations
                .push(your_language::diagnostic::DiagnosticAnnotation {
                    span: Span::new("other", 1, 2),
                    label: Some("related".into()),
                });
            Err(diagnostic)
        }
    }
    let error = program.run(&mut Fails).unwrap_err();
    assert_eq!(*error.primary, Span::new("input", 0, 5));
    assert_eq!(error.annotations[0].span.file, "other");
}
#[test]
fn parser_checks_and_editor_inspection_do_not_invoke_hooks() {
    let language = language(SIMPLE);
    assert!(!your_language::parse_named(&language, "input", "1,2").has_errors());
    let analysis =
        your_language::semantics::SemanticEngine::default().analyze(&language, "input", "1,2");
    assert!(analysis.diagnostics.is_empty());
    assert!(!analysis.occurrences.is_empty());
    let _ = language.highlight_named("input", "1,2");
}
#[test]
fn backend_can_inspect_static_facts_without_owning_analysis_state() {
    let language = language(
        r#"import { contribute } from "core/query" symbol tag hook literal(value) node Program = value:/[0-9]+/ { semantics { contribute((self, tag), "checked") } backend { literal(value) } } entry Program"#,
    );
    let program = language.prepare_backend("input", "42").unwrap();
    let identity = program.semantic_identity(program.root()).unwrap();
    let key = your_language::semantics::queries::QueryKey::Tuple(vec![
        your_language::semantics::queries::QueryKey::Identity(identity),
        your_language::semantics::queries::QueryKey::Symbol("main.yl#tag".into()),
    ]);
    assert!(program
        .analysis()
        .graph
        .query_entries
        .iter()
        .any(|entry| entry.key == key
            && entry.value == your_language::semantics::SemanticValue::Text("checked".into())));
    assert_eq!(program.run(&mut Host::default()).unwrap(), "42");
}
#[test]
fn cli_inspects_hook_contracts_without_a_host_or_definition_sources() {
    let language = language(SIMPLE);
    let artifact =
        std::env::temp_dir().join(format!("yl-backend-manifest-{}.ylc", std::process::id()));
    std::fs::write(&artifact, language.to_bytes().unwrap()).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_yl"))
        .args(["language", artifact.to_str().unwrap(), "hooks", "--json"])
        .output()
        .unwrap();
    std::fs::remove_file(&artifact).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let hooks: Vec<HookContract> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(hooks, language.required_hooks());
}

fn hook_ids(language: &your_language::CompiledLanguage) -> Vec<&str> {
    language
        .required_hooks()
        .iter()
        .map(|hook| hook.id.as_str())
        .collect()
}

#[test]
fn composed_hook_modules_only_require_the_entry_grammar_subset() {
    let sources = BTreeMap::from([
        (
            "main.yl".into(),
            r#"
                import * as hooks from "./hooks"
                node Number = value: /[0-9]+/ { backend { hooks::number(value) } }
                node String = value: /[a-z]+/ { backend { hooks::string(value) } }
                node Program = left: Number "+" right: Number {
                    backend { hooks::add(left, right) }
                }
                entry Program
            "#
            .into(),
        ),
        (
            "hooks.yl".into(),
            "export hook aaaUnused() export hook add(left, right) export hook number(value) export hook string(value)"
                .into(),
        ),
    ]);
    let compiled = compile_sources("main.yl", &sources).unwrap();
    assert_eq!(hook_ids(&compiled), ["hooks.yl#add", "hooks.yl#number"]);
    let bytes = compiled.to_bytes().unwrap();
    let language = load_compiled_language(&bytes).unwrap();
    assert_eq!(language.to_bytes().unwrap(), bytes);
    assert_eq!(language.required_hooks(), compiled.required_hooks());
    let image: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(image["backend"]["calls"].get("main.yl#String").is_none());
    // Pruning hook requirements must not remove unrelated grammar/tooling data.
    assert!(image["parser"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["definition"] == "main.yl#String"));

    struct Integers;
    impl Backend for Integers {
        type Value = u32;
        fn bindings(&self) -> Vec<HookBinding> {
            vec![
                HookBinding::new("hooks.yl#number", &["value"]),
                HookBinding::new("hooks.yl#add", &["left", "right"]),
            ]
        }
        fn invoke(
            &mut self,
            cx: &mut BackendContext<'_, '_, Self>,
            hook: &HookContract,
            args: &[Capture],
        ) -> Result<u32, Diagnostic> {
            match hook.id.as_str() {
                "hooks.yl#number" => Ok(cx.program().text(args[0].syntax()?)?.parse().unwrap()),
                "hooks.yl#add" => {
                    Ok(cx.dispatch(self, args[0].syntax()?)?
                        + cx.dispatch(self, args[1].syntax()?)?)
                }
                _ => unreachable!("Only the integer hook subset is bound"),
            }
        }
    }
    assert_eq!(
        language
            .prepare_backend("input.txt", "12+30")
            .unwrap()
            .run(&mut Integers)
            .unwrap(),
        42
    );

    let mut string_sources = sources;
    let source = string_sources.get_mut("main.yl").unwrap();
    *source = source.replace("entry Program", "entry String");
    let strings = compile_sources("main.yl", &string_sources).unwrap();
    assert_eq!(hook_ids(&strings), ["hooks.yl#string"]);
}

#[test]
fn reachable_alternatives_are_required_before_any_host_effects() {
    let language = language(
        r#"
            hook literal(value)
            hook branch(value)
            hook unused()
            node Number = value: /[0-9]+/ { backend { literal(value) } }
            node Other = value: /[a-z]+/ { backend { branch(value) } }
            node Expression { node Number node Other }
            node Unused = "!" { backend { unused() } }
            entry Expression
        "#,
    );
    assert_eq!(hook_ids(&language), ["main.yl#branch", "main.yl#literal"]);
    // This source does not select Other. Its hook is nevertheless a requirement
    // of the language, rather than a source-specific late dispatch error.
    let program = language.prepare_backend("input", "42").unwrap();
    let mut host = Host::default();
    assert_eq!(
        program.run(&mut host).unwrap_err().code,
        "yl.backend_binding"
    );
    assert_eq!(host.calls, 0);
}

#[test]
fn selecting_a_member_alias_does_not_make_its_siblings_reachable() {
    let language = language(
        r#"
            hook literal(value)
            hook unused()
            node Number = value: /[0-9]+/ { backend { literal(value) } }
            node Family {
                node Number
                node Other = "!" { backend { unused() } }
            }
            node Outer { node Family }
            entry Outer::Family::Number
        "#,
    );
    assert_eq!(hook_ids(&language), ["main.yl#literal"]);
    assert_eq!(
        language
            .prepare_backend("input", "42")
            .unwrap()
            .run(&mut Host::default())
            .unwrap(),
        "42"
    );
}

#[test]
fn declarations_without_reachable_mappings_do_not_require_bindings() {
    let language = language("hook unused() node Program = \"42\" entry Program");
    assert!(language.required_hooks().is_empty());
    let image: serde_json::Value = serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    assert_eq!(image["backend"]["hooks"], serde_json::json!([]));
    assert_eq!(image["backend"]["calls"], serde_json::json!({}));
}

#[test]
fn recursive_abstract_operators_and_parameterized_patterns_are_reachable() {
    let language = language(
        r#"
            import { separatedBy } from "std/parser"
            hook literal(value)
            hook pair(left, right)
            hook collect(values)
            hook unused()
            pattern Wrapped(item) = value: item { backend { collect([value]) } }
            node Unused = "unused" { backend { unused() } }
            node Expression {
                node Addition = left: Expression "+" right: Expression {
                    backend { pair(left, right) }
                }
                node Number = value: /[0-9]+/ { backend { literal(value) } }
                node Group = "(" items: Wrapped(Expression)* |> separatedBy(",") ")" {
                    backend { collect(items) }
                }
                precedence { Addition }
            }
            entry Expression
        "#,
    );
    assert_eq!(
        hook_ids(&language),
        ["main.yl#collect", "main.yl#literal", "main.yl#pair"]
    );
    let image: serde_json::Value = serde_json::from_slice(&language.to_bytes().unwrap()).unwrap();
    assert!(image["backend"]["calls"].get("main.yl#Wrapped").is_some());
    assert!(image["backend"]["calls"].get("main.yl#Unused").is_none());
    assert!(!your_language::parse_named(&language, "input", "(1,2+3)").has_errors());
}

#[test]
fn preview_only_patterns_and_assertion_probes_do_not_require_hooks() {
    let language = language(
        r#"
            import { notAhead } from "core/parser"
            hook literal(value)
            hook unused()
            pattern PreviewOnly = "preview" { backend { unused() } }
            node PreviewRoot = PreviewOnly { ide { example "preview" } }
            node Probe = "!" { backend { unused() } }
            node Program = notAhead(Probe) value: /[0-9]+/ {
                backend { literal(value) }
            }
            entry Program
        "#,
    );
    assert_eq!(hook_ids(&language), ["main.yl#literal"]);
    let program = language.prepare_backend("input", "42").unwrap();
    assert_eq!(program.run(&mut Host::default()).unwrap(), "42");
}

#[test]
fn malformed_unreachable_mappings_are_still_compile_errors() {
    for (mapping, code) in [
        ("literal()", "yl.backend_arity"),
        ("literal(missing)", "yl.artifact"),
    ] {
        let source = format!(
            "hook literal(value) node Unused = \"!\" {{ backend {{ {mapping} }} }} \
             node Program = \"42\" entry Program"
        );
        let errors =
            compile_sources("main.yl", &BTreeMap::from([("main.yl".into(), source)])).unwrap_err();
        assert_eq!(errors[0].code, code);
    }
}
