use std::collections::BTreeMap;

pub const ENTRY: &str = "definition/language.yl";

pub fn sources() -> BTreeMap<String, String> {
    BTreeMap::from([
        (ENTRY.into(), include_str!("definition/language.yl").into()),
        (
            "definition/lexical.yl".into(),
            include_str!("definition/lexical.yl").into(),
        ),
        (
            "definition/expression.yl".into(),
            include_str!("definition/expression.yl").into(),
        ),
        (
            "definition/function.yl".into(),
            include_str!("definition/function.yl").into(),
        ),
        (
            "definition/statements.yl".into(),
            include_str!("definition/statements.yl").into(),
        ),
    ])
}
