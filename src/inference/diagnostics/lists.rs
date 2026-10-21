//! List diagnostics observe each immutable round before new facts are published.

use oxigraph::model::*;

use super::rdf_list::{ListWitnesses, RdfList};
use super::{Diagnostics, has, objects, text};
use crate::Result;
use crate::graph::{RDF, iri};
use crate::inference::OWL;
use crate::inference::backend::Graph;

#[derive(Clone, Copy)]
enum Kind {
    Different,
    Properties,
    Classes,
}

pub(super) fn collect(
    data: &impl Graph,
    errors: &mut Diagnostics,
    witnesses: &ListWitnesses,
) -> Result<()> {
    // bounded: three built-in declaration types and their two possible list properties.
    for (name, kind) in [
        ("AllDifferent", Kind::Different),
        ("AllDisjointProperties", Kind::Properties),
        ("AllDisjointClasses", Kind::Classes),
    ] {
        let class: Term = iri(OWL, name).into();
        for declaration in data.quads(None, Some(iri(RDF, "type").as_ref()), Some(class.as_ref())) {
            let node: Term = declaration?.subject.into();
            for property in ["members", "distinctMembers"] {
                if property == "distinctMembers" && !matches!(kind, Kind::Different) {
                    continue;
                }
                for head in objects(data, &node, &iri(OWL, property)) {
                    check(data, errors, RdfList::new(data, witnesses, head?)?, kind)?;
                }
            }
        }
    }
    Ok(())
}

fn check(
    data: &impl Graph,
    errors: &mut Diagnostics,
    list: RdfList<'_, impl Graph>,
    kind: Kind,
) -> Result<()> {
    let mut outer = list.cursor();
    for index in 0..list.len.saturating_sub(1) {
        let Some(left) = outer.next()? else {
            break;
        };
        let mut inner = outer.tail();
        // Preserve OWL-RL 7.6.2's exclusion of the final element in these two
        // loops; AllDisjointClasses intentionally uses the complete suffix.
        let end = if matches!(kind, Kind::Classes) {
            list.len
        } else {
            list.len - 1
        };
        for _ in index + 1..end {
            let Some(right) = inner.next()? else {
                break;
            };
            match kind {
                Kind::Different => {
                    let same = iri(OWL, "sameAs");
                    if left != right
                        && (has(data, &left, &same, &right)? || has(data, &right, &same, &left)?)
                    {
                        errors.add(format!("'sameAs' and 'AllDifferent' cannot be used on the same subject-object pair: ({}, {})", text(&left), text(&right)));
                    }
                }
                Kind::Properties => properties(data, errors, &left, &right)?,
                Kind::Classes => classes(data, errors, &left, &right)?,
            }
        }
    }
    Ok(())
}

fn properties(
    data: &impl Graph,
    errors: &mut Diagnostics,
    left: &Term,
    right: &Term,
) -> Result<()> {
    let (Term::NamedNode(left), Term::NamedNode(right)) = (left, right) else {
        return Ok(());
    };
    for quad in data.quads(None, Some(left.as_ref()), None) {
        let quad = quad?;
        let subject: Term = quad.subject.into();
        if has(data, &subject, right, &quad.object)? {
            errors.add(format!(
                "Disjoint properties in an 'AllDisjointProperties' are not really disjoint: ({}, {},{}) and ({},{},{})",
                text(&subject), left.as_str(), text(&quad.object), text(&subject), right.as_str(), text(&quad.object)
            ));
        }
    }
    Ok(())
}

fn classes(data: &impl Graph, errors: &mut Diagnostics, left: &Term, right: &Term) -> Result<()> {
    let predicate = iri(RDF, "type");
    for quad in data.quads(None, Some(predicate.as_ref()), Some(left.as_ref())) {
        let subject: Term = quad?.subject.into();
        if has(data, &subject, &predicate, right)? {
            errors.add(format!(
                "Disjoint classes {} and {} have a common individual {}",
                text(left),
                text(right),
                text(&subject)
            ));
        }
    }
    Ok(())
}
