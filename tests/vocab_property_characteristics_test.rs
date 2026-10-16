use open_ontologies::{graph::GraphStore, vocab_check::check_data_vocab};
use serde_json::Value;
use std::sync::Arc;

#[test]
fn characteristic_only_properties_are_declared_vocabulary() {
    for kind in [
        "TransitiveProperty",
        "SymmetricProperty",
        "AsymmetricProperty",
        "ReflexiveProperty",
        "IrreflexiveProperty",
    ] {
        let ontology = Arc::new(GraphStore::new());
        ontology
            .load_turtle(
                &format!(
                    "@prefix owl: <http://www.w3.org/2002/07/owl#> .
             @prefix ex: <http://example.org/onto#> .
             ex:related a owl:{kind} ."
                ),
                None,
            )
            .unwrap();
        let data = "<http://example.org/alice> <http://example.org/onto#related> <http://example.org/bob> .";
        let report: Value =
            serde_json::from_str(&check_data_vocab(&ontology, data, &[]).unwrap()).unwrap();
        assert_eq!(report["conforms"], true, "{kind}: {report}");
        assert_eq!(report["ontology_terms"], 1);
        assert_eq!(
            report["checked_namespaces"],
            serde_json::json!(["http://example.org/onto#"])
        );
        assert_eq!(report["hallucinated_terms"], serde_json::json!([]));

        let bad: Value = serde_json::from_str(
            &check_data_vocab(
                &ontology,
                &data.replace("onto#related", "onto#invented"),
                &[],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(bad["conforms"], false);
        assert_eq!(
            bad["hallucinated_terms"],
            serde_json::json!(["http://example.org/onto#invented"])
        );
    }
}

#[test]
fn custom_property_like_type_is_not_a_declaration() {
    let ontology = Arc::new(GraphStore::new());
    ontology
        .load_turtle(
            "@prefix ex: <http://example.org/onto#> . ex:related a ex:CustomProperty .",
            None,
        )
        .unwrap();
    let report: Value = serde_json::from_str(
        &check_data_vocab(
            &ontology,
            "<http://example.org/a> <http://example.org/onto#related> <http://example.org/b> .",
            &[],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(report["conforms"], false);
    assert_eq!(report["ontology_terms"], 0);
    assert!(
        report["warning"]
            .as_str()
            .unwrap()
            .contains("nothing was checked")
    );
}
