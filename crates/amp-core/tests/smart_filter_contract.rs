use amp_core::smart_filters::{Field, Kind};

#[test]
fn published_filter_contract_matches_the_shared_engine() {
    let contract: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/smart_fields.json")).unwrap();
    assert_eq!(contract["schema"], 1);
    let actual: Vec<_> = Field::ALL
        .iter()
        .map(|field| {
            let kind = match field.kind() {
                Kind::Text => "Text",
                Kind::Flag => "Flag",
                Kind::Number(_, _) => "Number",
            };
            serde_json::json!({"field":field,"label":field.label(),"kind":kind})
        })
        .collect();
    assert_eq!(contract["fields"], serde_json::json!(actual));
}
