use super::*;

impl Evaluation {
    pub(super) fn new(max_depth: usize) -> Self {
        Self {
            path_values: HashMap::new(),
            memberships: HashMap::new(),
            class_targets: HashMap::new(),
            cached_terms: 0,
            functions: Functions::default(),
            path: Vec::new(),
            max_depth,
            warnings: Vec::new(),
            batch: None,
        }
    }

    pub(super) fn clear_traversals(&mut self) {
        self.cached_terms = 0;
        self.path_values.clear();
        self.memberships.clear();
        self.class_targets.clear();
    }

    pub(super) fn reserve_traversal(&mut self, terms: usize) -> bool {
        if terms > 32768 {
            return false;
        }
        if self.path_values.len() + self.memberships.len() + self.class_targets.len() >= 1024
            || self.cached_terms + terms > 32768
        {
            self.clear_traversals();
        }
        self.cached_terms += terms;
        true
    }

    pub(super) fn values(
        &mut self,
        path: &Path,
        data: &(impl GraphRead + ?Sized),
        focus: &Term,
    ) -> Result<Vec<Term>> {
        if matches!(path, Path::Predicate(_)) {
            return path.values(data, focus);
        }
        let key = (path.clone(), focus.clone());
        if let Some(values) = self.path_values.get(&key) {
            return Ok(values.clone());
        }
        let values = path.values(data, focus)?;
        if self.reserve_traversal(values.len()) {
            self.path_values.insert(key, values.clone());
        }
        Ok(values)
    }

    pub(super) fn is_instance(
        &mut self,
        data: &(impl GraphRead + ?Sized),
        value: &Term,
        class: &Term,
    ) -> Result<bool> {
        let key = (value.clone(), class.clone());
        if let Some(result) = self.memberships.get(&key) {
            return Ok(*result);
        }
        let result = is_instance(data, value, class)?;
        if self.reserve_traversal(1) {
            self.memberships.insert(key, result);
        }
        Ok(result)
    }

    pub(super) fn targets(
        &mut self,
        data: &(impl GraphRead + ?Sized),
        class: &Term,
    ) -> Result<HashSet<Term>> {
        if let Some(nodes) = self.class_targets.get(class) {
            return Ok(nodes.clone());
        }
        let nodes = instances(data, class)?;
        if self.reserve_traversal(nodes.len()) {
            self.class_targets.insert(class.clone(), nodes.clone());
        }
        Ok(nodes)
    }

    pub(super) fn describe_path(
        &self,
        shapes: &[Shape],
        current: Option<EvaluationStep>,
    ) -> String {
        self.path
            .iter()
            .copied()
            .chain(current)
            .map(|step| format!("{} -> {}", shapes[step.shape].node, step.component))
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    /// Match pySHACL's recursion_triggers: shape/constraint edges, not RDF nodes.
    pub(super) fn skip_recursive(
        &mut self,
        shape: usize,
        component: &'static str,
        target: usize,
        shapes: &[Shape],
    ) -> bool {
        let alternating = self.path.last().is_some_and(|previous| {
            matches!(
                (previous.component, component),
                ("NodeConstraintComponent", "PropertyConstraintComponent")
                    | ("PropertyConstraintComponent", "NodeConstraintComponent")
            )
        });
        let depth = self.path.len() + 1;
        if depth < if alternating { 6 } else { 3 } {
            return false;
        }
        let recursive = self.path.iter().enumerate().any(|(i, step)| {
            step.shape == shape
                && step.component == component
                && self.path.get(i + 1).map_or(shape, |next| next.shape) == target
        });
        if recursive {
            let path = self.describe_path(shapes, Some(EvaluationStep { shape, component }));
            self.warnings.push(format!(
                "Warning, A Recursive Shape was detected executing a recursive validation sequence {} levels deep. Backing out.\n{path}\nFor reference, see https://www.w3.org/TR/shacl/#shapes-recursion",
                depth * 2
            ));
        }
        recursive
    }
}

pub(super) fn is_instance(
    data: &(impl GraphRead + ?Sized),
    value: &Term,
    class: &Term,
) -> Result<bool> {
    let mut pending = objects(data, value, &iri(RDF, "type"))?;
    let mut seen = HashSet::new();
    while let Some(current) = pending.pop() {
        if &current == class {
            return Ok(true);
        }
        if seen.insert(current.clone()) {
            pending.extend(objects(data, &current, &iri(RDFS, "subClassOf"))?);
        }
    }
    Ok(false)
}
