//! Weak connectivity bounds candidate searches; it never proves an equality fact.
use std::collections::HashMap;

use oxigraph::model::Term;

#[derive(Default)]
pub(super) struct Components {
    ids: HashMap<Term, usize>,
    parents: Vec<usize>,
    sizes: Vec<usize>,
    members: Vec<Vec<Term>>,
}

impl Components {
    fn id(&mut self, term: Term) -> usize {
        let next = self.parents.len();
        *self.ids.entry(term).or_insert_with(|| {
            self.parents.push(next);
            self.sizes.push(1);
            next
        })
    }

    fn root(&self, mut id: usize) -> usize {
        while self.parents[id] != id {
            id = self.parents[id];
        }
        id
    }

    pub(super) fn connect(&mut self, left: Term, right: Term) {
        let left = self.id(left);
        let right = self.id(right);
        let (mut left, mut right) = (self.root(left), self.root(right));
        if left == right {
            return;
        }
        if self.sizes[left] < self.sizes[right] {
            std::mem::swap(&mut left, &mut right);
        }
        self.parents[right] = left;
        self.sizes[left] += self.sizes[right];
    }

    pub(super) fn finish(&mut self) {
        self.members.resize_with(self.parents.len(), Vec::new);
        for (term, &id) in &self.ids {
            let root = self.root(id);
            self.members[root].push(term.clone());
        }
    }

    pub(super) fn members(&self, term: &Term) -> &[Term] {
        &self.members[self.root(self.ids[term])]
    }
}
