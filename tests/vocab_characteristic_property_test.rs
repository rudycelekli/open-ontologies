use open_ontologies::graph::GraphStore;
use open_ontologies::vocab_check::check_data_vocab;
use std::sync::Arc;

#[test]
fn characteristic_properties_declare_vocabulary_without_redundant_types() {
    let prefix =
        "@prefix ex: <http://example.org/> . @prefix owl: <http://www.w3.org/2002/07/owl#> . ";
    for kind in ["TransitiveProperty", "SymmetricProperty"] {
        for with_class in [false, true] {
            let onto = Arc::new(GraphStore::new());
            let mut text = format!("{prefix}ex:related a owl:{kind} .");
            if with_class {
                text.push_str(" ex:Thing a owl:Class .");
            }
            onto.load_turtle(&text, None).unwrap();
            let valid =
                check_data_vocab(&onto, &format!("{prefix}ex:a ex:related ex:b ."), &[]).unwrap();
            let report: serde_json::Value = serde_json::from_str(&valid).unwrap();
            assert_eq!(report["conforms"], true, "{kind}: {report}");
            assert_eq!(report["ontology_terms"], 1 + u64::from(with_class));
            let unknown =
                check_data_vocab(&onto, &format!("{prefix}ex:a ex:invented ex:b ."), &[]).unwrap();
            let report: serde_json::Value = serde_json::from_str(&unknown).unwrap();
            assert_eq!(report["conforms"], false);
            assert_eq!(
                report["hallucinated_terms"],
                serde_json::json!(["http://example.org/invented"])
            );
        }
    }
}
