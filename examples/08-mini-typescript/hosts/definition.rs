use std::collections::BTreeMap;
pub const ENTRY: &str = "definition/language.yl";
pub fn sources() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "definition/typing.yl".into(),
            include_str!("../definition/typing.yl").into(),
        ),
        (
            "definition/language.yl".into(),
            include_str!("../definition/language.yl").into(),
        ),
        (
            "definition/lexical.yl".into(),
            include_str!("../definition/lexical.yl").into(),
        ),
        (
            "definition/types.yl".into(),
            include_str!("../definition/types.yl").into(),
        ),
        (
            "definition/parameter.yl".into(),
            include_str!("../definition/parameter.yl").into(),
        ),
        (
            "definition/expression.yl".into(),
            include_str!("../definition/expression.yl").into(),
        ),
        (
            "definition/function.yl".into(),
            include_str!("../definition/function.yl").into(),
        ),
        (
            "definition/let.yl".into(),
            include_str!("../definition/let.yl").into(),
        ),
    ])
}
