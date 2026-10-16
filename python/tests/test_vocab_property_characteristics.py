"""Explicit OWL property characteristics still declare usable vocabulary."""

import pytest

from open_ontologies_lite.vocab_check import vocab_check


@pytest.mark.parametrize("characteristic", [
    "TransitiveProperty", "SymmetricProperty", "AsymmetricProperty",
    "ReflexiveProperty", "IrreflexiveProperty",
])
def test_characteristic_declares_property_without_object_property_repetition(characteristic):
    ontology = f"""
        @prefix owl: <http://www.w3.org/2002/07/owl#> .
        @prefix ex: <http://example.org/onto#> .
        ex:related a owl:{characteristic} .
    """
    data = """
        @prefix ex: <http://example.org/onto#> .
        ex:alice ex:related ex:bob .
    """
    report = vocab_check(ontology, data)
    assert report["conforms"] is True, report
    assert report["ontology_terms"] == 1
    assert report["checked_namespaces"] == ["http://example.org/onto#"]
    assert report["undeclared_terms"] == []

    # Recognition must not exempt other terms in the same namespace.
    invented = vocab_check(ontology, data.replace("ex:related", "ex:invented"))
    assert invented["conforms"] is False
    assert invented["undeclared_terms"] == ["http://example.org/onto#invented"]


def test_arbitrary_property_like_type_does_not_declare_vocabulary():
    ontology = """
        @prefix ex: <http://example.org/onto#> .
        ex:related a ex:CustomProperty .
    """
    report = vocab_check(ontology, "<http://example.org/a> <http://example.org/onto#related> <http://example.org/b> .")
    assert report["conforms"] is False
    assert report["ontology_terms"] == 0
    assert "nothing was checked" in report["warning"]
