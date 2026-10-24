use super::*;
use oxigraph::io::RdfFormat;

fn graph(body: &str) -> Dataset {
    crate::parse_graph(
        &format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#>. @prefix ex: <https://example.org/>. {body}"
        ),
        RdfFormat::Turtle,
        None,
    )
    .unwrap()
}

#[test]
fn component_registration_rejects_missing_reserved_duplicate_and_invalid_parameters() {
    for body in [
        "ex:C a sh:ConstraintComponent; sh:parameter [].",
        "ex:C a sh:ConstraintComponent; sh:parameter [sh:path ex:this].",
        "ex:C a sh:ConstraintComponent; sh:parameter [sh:path ex:value].",
        "ex:C a sh:ConstraintComponent; sh:parameter [sh:path <https://example.org/bad-name>].",
        "ex:C a sh:ConstraintComponent; sh:parameter [sh:path ex:limit], [sh:path <urn:other/limit>].",
        "ex:C a sh:ConstraintComponent; sh:parameter [sh:path ex:limit; sh:optional true].",
    ] {
        assert!(Component::discover(&graph(body)).is_err(), "{body}");
    }
}

#[test]
fn active_components_reject_missing_binding_records_and_empty_required_values() {
    let data = graph(
        "ex:C a sh:ConstraintComponent; sh:parameter [sh:path ex:limit]; sh:validator [sh:ask 'ASK {}'].",
    );
    let components = Component::discover(&data).unwrap();
    let component = &components[0];
    let mut application = model::CustomConstraint {
        component: NamedNode::new("https://example.org/C").unwrap(),
        parameters: vec![],
    };
    assert!(
        component
            .lower(&data, application.clone(), None, &[])
            .err()
            .unwrap()
            .to_string()
            .contains("Missing parameter binding")
    );
    application.parameters.push(model::ParameterValues {
        path: component.parameters[0].declaration.path.clone(),
        values: vec![],
    });
    assert!(
        component
            .lower(&data, application, None, &[])
            .err()
            .unwrap()
            .to_string()
            .contains("requires its parameter values")
    );
}
