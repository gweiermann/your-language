use super::*;
use crate::semantics::queries::{QueryAction, QueryExpr};
fn compile(source: &str) -> CompileResult<CompiledLanguage> {
    let mut registry = OperationRegistry::new();
    crate::semantics::queries::register(&mut registry).unwrap();
    compile_sources_with_registry(
        "query.yl",
        &BTreeMap::from([("query.yl".into(), source.into())]),
        &registry,
    )
}
#[test]
fn query_wrappers_expand_to_normalized_callbacks_and_conditionals() {
    let source = r#"import { error, warn, hint } from "core/diagnostics"
import { get, contribute, ancestors, find, isPresent, text } from "core/query"
symbol scope
symbol declarations
operation resolve(target, name) = find(ancestors(target), ancestor => get((get((ancestor, scope)), declarations, text(name))))
operation declareName(target, name) {
when isPresent(name) { contribute((target, declarations, text(name)), target) }
}
node Program = name: /[a-z]+/ {
semantics {
    group Register { declareName(self, name) }
    when !isPresent(resolve(self, name)) { error("Missing ${name}") }
}
}
entry Program
"#;
    let language = compile(source).unwrap();
    let groups = &language.query_meanings["query.yl#Program"];
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].name.as_deref(), Some("Register"));
    let QueryAction::If {
        condition: QueryExpr::Not(value),
        ..
    } = &groups[1].actions[0]
    else {
        panic!("expected if");
    };
    let QueryExpr::Call { name, args } = &**value else {
        panic!("expected isPresent");
    };
    assert_eq!(name, "core/query#isPresent");
    let QueryExpr::Call { args, .. } = &args[0] else {
        panic!("expected find");
    };
    let QueryExpr::Lambda { parameter, body } = &args[1] else {
        panic!("expected callback");
    };
    assert!(parameter.starts_with("ancestor@"));
    assert!(serde_json::to_string(body).unwrap().contains("Local"));
    let bytes = language.to_bytes().unwrap();
    let loaded = crate::load_compiled_language(&bytes).unwrap();
    assert_eq!(
        language.executable_image().queries,
        loaded.executable_image().queries
    );
}
#[test]
fn pure_query_helpers_and_nested_callbacks_keep_lexical_bindings() {
    let language = compile(r#"import { contribute, find, ancestors, get } from "core/query"
operation key(value) = (value, "tag")
operation select(target, name) = find(ancestors(target), outer => find(ancestors(outer), inner => get((outer, inner, name))))
node Program = name: /[a-z]+/ { semantics { contribute(key(self), select(self, name)) } }
entry Program"#).unwrap();
    let QueryAction::Call { args, .. } = &language.query_meanings["query.yl#Program"][0].actions[0]
    else {
        panic!("expected contribution");
    };
    assert_eq!(
        args[0],
        QueryExpr::Tuple(vec![QueryExpr::SelfValue, QueryExpr::Text("tag".into())])
    );
    let QueryExpr::Call { args, .. } = &args[1] else {
        panic!("expected find");
    };
    let QueryExpr::Lambda {
        parameter: outer,
        body,
    } = &args[1]
    else {
        panic!("expected outer callback");
    };
    let QueryExpr::Call { args, .. } = &**body else {
        panic!("expected nested find");
    };
    let QueryExpr::Lambda {
        parameter: inner,
        body,
    } = &args[1]
    else {
        panic!("expected inner callback");
    };
    let QueryExpr::Call { args, .. } = &**body else {
        panic!("expected get");
    };
    assert_eq!(
        args[0],
        QueryExpr::Tuple(vec![
            QueryExpr::Local(outer.clone()),
            QueryExpr::Local(inner.clone()),
            QueryExpr::Capture("name".into())
        ])
    );
}
#[test]
fn diagnostics_interpolate_bound_values_and_nested_queries() {
    use crate::semantics::queries::QueryMessagePart as P;
    let language = compile(r#"import { error, warn, hint } from "core/diagnostics"
import { get } from "core/query"
operation require(target, actual) { when !actual { error("Bad ${actual}; attachment ${get((target, \"}tag\"))}") } }
node Program = "x" { semantics { require(self, false) } }
entry Program"#).unwrap();
    let QueryAction::If { then_actions, .. } =
        &language.query_meanings["query.yl#Program"][0].actions[0]
    else {
        panic!("expected if");
    };
    let QueryAction::Emit { message, .. } = &then_actions[0] else {
        panic!("expected diagnostic");
    };
    assert_eq!(message[1], P::Value(QueryExpr::Bool(false)));
    assert_eq!(
        message[3],
        P::Value(QueryExpr::Call {
            name: "core/query#get".into(),
            args: vec![QueryExpr::Tuple(vec![
                QueryExpr::SelfValue,
                QueryExpr::Text("}tag".into())
            ])]
        })
    );
}
#[test]
fn malformed_diagnostic_expressions_are_structured_errors() {
    for message in ["${get(}", "${}", "${self self}"] {
        let source = format!(
            r#"import {{ error, warn, hint }} from "core/diagnostics"
node Program = "x" {{ semantics {{ when true {{ error("{message}") }} }} }} entry Program"#
        );
        let errors = compile(&source).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.code == "yl.invalid_diagnostic"),
            "{errors:?}"
        );
    }
}
#[test]
fn named_query_arguments_preserve_contract_order() {
    let language = compile(
        r#"import { contribute } from "core/query"
node Program = "x" { semantics { contribute(value = "result", key = self) } }
entry Program"#,
    )
    .unwrap();
    let QueryAction::Call { args, .. } = &language.query_meanings["query.yl#Program"][0].actions[0]
    else {
        panic!("expected call");
    };
    assert_eq!(
        args,
        &[QueryExpr::SelfValue, QueryExpr::Text("result".into())]
    );
}
#[test]
fn excess_and_duplicate_query_arguments_are_diagnostics() {
    for call in [
        "contribute(self)",
        "contribute(self, self, self)",
        "contribute(key = self, key = self, value = self)",
    ] {
        let source = format!("import {{ contribute }} from \"core/query\" node Program = \"x\" {{ semantics {{ {call} }} }} entry Program");
        let errors = compile(&source).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.code == "yl.invalid_argument"),
            "{errors:?}"
        );
    }
}
