//! Catalogue-specific guards remove heads that are already positive body facts.
use super::*;

impl CompiledRule {
    pub(super) fn without_self_subproperty(rule: &Rule) -> Result<Rule> {
        let invalid = || Error::Unsupported("Unexpected subproperty catalogue shape".into());
        let [Literal::Positive(schema), Literal::Positive(assertion)] = rule.body.as_slice() else {
            return Err(invalid());
        };
        for atom in [&rule.head, schema, assertion] {
            if atom.predicate.as_str() != TRIPLE_RELATION || atom.terms.len() != 3 {
                return Err(invalid());
            }
        }
        let source = &assertion.terms[1];
        let target = &rule.head.terms[1];
        if !matches!(
            &schema.terms[1],
            Term::Constant(iri)
                if iri.as_str() == "http://www.w3.org/2000/01/rdf-schema#subPropertyOf"
        ) || &schema.terms[0] != source
            || &schema.terms[2] != target
            || rule.head.terms[0] != assertion.terms[0]
            || rule.head.terms[2] != assertion.terms[2]
        {
            return Err(invalid());
        }
        // bounded: the four variables in the checked, fixed Rdfs7 clause.
        let variables = [&assertion.terms[0], source, &assertion.terms[2], target];
        for (index, variable) in variables.iter().enumerate() {
            if !matches!(variable, Term::Variable(_)) || variables[index + 1..].contains(variable) {
                return Err(invalid());
            }
        }
        // Equal predicates make the head identical to the assertion body fact.
        // Pending rejects it from the read union even for a separate destination.
        let mut optimized = rule.clone();
        optimized.body.push(Literal::Filter(BuiltinFilter::Ne(
            source.clone(),
            target.clone(),
        )));
        Ok(optimized)
    }
}
