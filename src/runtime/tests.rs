use super::*;

#[test]
fn parsing_and_highlighting_respect_bounds_on_a_one_mib_host_stack() {
    let language = crate::compile_sources(
        "language.yl",
        &BTreeMap::from([(
            "language.yl".into(),
            r#"node Program = "(" child:Program ")" | value:"x" {
                ide { highlightAs "type" }
            }
            entry Program"#
                .into(),
        )]),
    )
    .unwrap();
    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || {
            let source = format!("{}x{}", "(".repeat(30), ")".repeat(30));
            assert!(!parse_named(&language, "program", &source).has_errors());
            assert_eq!(language.highlight_named("program", &source).len(), 61);
            let oversized = format!("{}x{}", "(".repeat(300), ")".repeat(300));
            let rejected = parse_named(&language, "program", &oversized);
            assert!(rejected.ast.is_none());
            assert_eq!(rejected.diagnostics[0].code, "parse.resource_limit");
        })
        .unwrap()
        .join()
        .unwrap();
}
