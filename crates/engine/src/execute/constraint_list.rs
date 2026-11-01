//! SHACL 1.2 draft list validation through the borrowed graph owner.

use super::data_list::DataList;
use super::*;

impl CompiledShapes {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_list_constraint(
        &self,
        index: usize,
        focus: &Term,
        values: &[Term],
        data: &(impl GraphRead + ?Sized),
        constraint: &Constraint,
        results: &mut Vec<Violation>,
        evaluation: &mut Evaluation,
    ) -> Result<()> {
        let component = match constraint {
            Constraint::MemberShape(_) => "MemberShapeConstraintComponent",
            Constraint::ListLength { minimum: true, .. } => "MinListLengthConstraintComponent",
            Constraint::ListLength { minimum: false, .. } => "MaxListLengthConstraintComponent",
            Constraint::UniqueMembers(_) => "UniqueMembersConstraintComponent",
            _ => unreachable!("list constraint dispatch"),
        };
        for value in values {
            let mut result =
                Violation::new(index, &self.shapes[index], focus, component, Some(value));
            result.data_description = DataDescription::Identity;
            result.message =
                "Value must be a well-formed SHACL list satisfying the declared list constraint."
                    .into();
            let Some(list) = DataList::try_new(data, value)? else {
                results.push(result);
                continue;
            };
            let valid = match constraint {
                Constraint::MemberShape(target) => {
                    for member in list.members() {
                        self.evaluate_child(
                            index,
                            component,
                            *target,
                            &member?,
                            data,
                            &mut result.details,
                            evaluation,
                            None,
                        )?;
                    }
                    result.details.is_empty()
                }
                Constraint::ListLength { minimum, count } => {
                    if *minimum {
                        list.length >= count.number
                    } else {
                        list.length <= count.number
                    }
                }
                Constraint::UniqueMembers(unique) => !unique || !list.has_duplicates()?,
                _ => unreachable!("list constraint dispatch"),
            };
            if !valid {
                results.push(result)
            }
        }
        Ok(())
    }
}
