//! Borrowed Core target stages; the compiled shape retains the only target AST.
use crate::compile::Target;
use oxigraph::model::{NamedNode, Term};

/// Call-local view preserving Core target designation and declaration order.
/// Consumers construct the actual source sets at their own acquisition boundary.
#[derive(Clone, Copy)]
pub struct SelectedTargetsRef<'a> {
    targets: &'a [Target],
}
impl<'a> SelectedTargetsRef<'a> {
    pub(crate) fn new(targets: &'a [Target]) -> Self {
        Self { targets }
    }
    pub(crate) fn native_targets(self) -> &'a [Target] {
        self.targets
    }
    /// Whether this view contains only the supported Core target families.
    /// Advanced declarations are explicit; their absence is never inferred from
    /// an empty discovery result.
    pub fn is_core(self) -> bool {
        self.targets
            .iter()
            .all(|t| !matches!(t, Target::Sparql { .. }))
    }
    /// Explicit target nodes, before class/subject/object set merging.
    pub fn nodes(self) -> impl Iterator<Item = &'a Term> + Clone {
        self.targets.iter().filter_map(|t| match t {
            Target::Node(node) => Some(node),
            _ => None,
        })
    }
    /// Explicit classes, used to construct the reference class set first.
    pub fn classes(self) -> impl Iterator<Item = &'a Term> + Clone {
        self.targets.iter().filter_map(|t| match t {
            Target::Class(class) => Some(class),
            _ => None,
        })
    }
    /// Implicit classes, merged in a distinct reference set-update stage.
    pub fn implicit_classes(self) -> impl Iterator<Item = &'a Term> + Clone {
        self.targets.iter().filter_map(|t| match t {
            Target::ImplicitClass(class) => Some(class),
            _ => None,
        })
    }
    /// Predicates whose actual source subject/object pairs supply subjects.
    pub fn subjects_of(self) -> impl Iterator<Item = &'a NamedNode> + Clone {
        self.targets.iter().filter_map(|t| match t {
            Target::Subjects(predicate) => Some(predicate),
            _ => None,
        })
    }
    /// Predicates whose actual source subject/object pairs supply objects.
    pub fn objects_of(self) -> impl Iterator<Item = &'a NamedNode> + Clone {
        self.targets.iter().filter_map(|t| match t {
            Target::Objects(predicate) => Some(predicate),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_target_view_preserves_each_stage_without_copying_terms() {
        let node = |s| NamedNode::new(s).unwrap();
        let targets = [
            Target::Class(node("urn:C").into()),
            Target::Node(node("urn:N").into()),
            Target::ImplicitClass(node("urn:I").into()),
            Target::Class(node("urn:E").into()),
            Target::Subjects(node("urn:S")),
            Target::Objects(node("urn:O")),
        ];
        let view = SelectedTargetsRef::new(&targets);
        assert!(view.is_core());
        let Target::Class(first) = &targets[0] else {
            unreachable!()
        };
        assert!(std::ptr::eq(view.classes().next().unwrap(), first));
        assert_eq!(
            view.classes().map(ToString::to_string).collect::<Vec<_>>(),
            ["<urn:C>", "<urn:E>"]
        );
        assert_eq!(
            view.implicit_classes().next().unwrap().to_string(),
            "<urn:I>"
        );
        assert_eq!(view.nodes().next().unwrap().to_string(), "<urn:N>");
        assert_eq!(view.subjects_of().next().unwrap().as_str(), "urn:S");
        assert_eq!(view.objects_of().next().unwrap().as_str(), "urn:O");
        assert_eq!(std::mem::size_of::<SelectedTargetsRef<'_>>(), 16);
        assert!(SelectedTargetsRef::new(&[]).is_core());
        assert_eq!(view.native_targets().len(), targets.len());
    }
}
