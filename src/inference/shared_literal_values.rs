//! Lower the checked one-time literal clause without rediscovering bound terms.

use super::*;

impl CompiledRule {
    pub(super) fn literal_value_parts(rule: &Rule) -> Result<(String, String)> {
        // Validate the complete original clause before specializing its known roles.
        let (head, _) = Self::validated_parts(rule, true)?;
        let invalid = || Error::Unsupported("Unexpected literal substitution clause shape".into());
        let [
            Literal::Positive(assertion),
            Literal::Positive(original),
            Literal::Positive(replacement),
        ] = rule.body.as_slice()
        else {
            return Err(invalid());
        };
        for atom in [assertion, &rule.head] {
            if atom.predicate.as_str() != TRIPLE_RELATION || atom.terms.len() != 3 {
                return Err(invalid());
            }
        }
        for atom in [original, replacement] {
            if atom.predicate.as_str() != LITERAL_VALUE_RELATION || atom.terms.len() != 2 {
                return Err(invalid());
            }
        }
        if rule.head.terms[..2] != assertion.terms[..2]
            || rule.head.terms[2] != replacement.terms[0]
            || assertion.terms[2] != original.terms[0]
            || original.terms[1] != replacement.terms[1]
        {
            return Err(invalid());
        }
        // bounded: exactly the five distinct variable roles in this fixed clause.
        let roles = [
            &assertion.terms[0],
            &assertion.terms[1],
            &original.terms[0],
            &replacement.terms[0],
            &original.terms[1],
        ];
        for (index, role) in roles.iter().enumerate() {
            if !matches!(role, Term::Variable(_)) || roles[..index].contains(role) {
                return Err(invalid());
            }
        }
        let literal = Self::term(&original.terms[0])?;
        let key = Self::term(&original.terms[1])?;
        let subject = Self::term(&assertion.terms[0])?;
        let predicate = Self::term(&assertion.terms[1])?;
        // The assertion has already bound the actual original object. The same
        // evaluator supplies its value key; undefined keys still remove the row.
        // Scope the guard before the join: otherwise the replacement side can
        // fill an undefined key and make an invalid original object pass BOUND.
        let body = format!(
            "{{ SELECT {subject} {predicate} {literal} {key} WHERE {{ {} BIND (<{LITERAL_VALUE_RELATION}>({literal}) AS {key}) FILTER (BOUND({key})) }} }} {}",
            Self::atom(assertion)?,
            Self::body_atom(replacement, true)?,
        );
        Ok((head, body))
    }
}
