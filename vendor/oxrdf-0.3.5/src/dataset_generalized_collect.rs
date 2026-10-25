//! Standard collection ownership paths, all using the same interner and indexes.
use super::*;

impl<T> FromIterator<T> for GeneralizedDataset
where
    Self: Extend<T>,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut data = Self::new();
        data.extend(iter);
        data
    }
}

impl Extend<Quad> for GeneralizedDataset {
    fn extend<I: IntoIterator<Item = Quad>>(&mut self, iter: I) {
        for quad in iter {
            self.insert(&quad);
        }
    }
}
impl<'a> Extend<&'a Quad> for GeneralizedDataset {
    fn extend<I: IntoIterator<Item = &'a Quad>>(&mut self, iter: I) {
        for quad in iter {
            self.insert(quad);
        }
    }
}
impl<'a> Extend<QuadRef<'a>> for GeneralizedDataset {
    fn extend<I: IntoIterator<Item = QuadRef<'a>>>(&mut self, iter: I) {
        for quad in iter {
            self.insert(quad);
        }
    }
}
impl Extend<GeneralizedQuad> for GeneralizedDataset {
    fn extend<I: IntoIterator<Item = GeneralizedQuad>>(&mut self, iter: I) {
        for quad in iter {
            self.insert(&quad);
        }
    }
}
impl<'a> Extend<&'a GeneralizedQuad> for GeneralizedDataset {
    fn extend<I: IntoIterator<Item = &'a GeneralizedQuad>>(&mut self, iter: I) {
        for quad in iter {
            self.insert(quad);
        }
    }
}
impl<'a> Extend<GeneralizedQuadRef<'a>> for GeneralizedDataset {
    fn extend<I: IntoIterator<Item = GeneralizedQuadRef<'a>>>(&mut self, iter: I) {
        for quad in iter {
            self.insert(quad);
        }
    }
}

impl PartialEq for GeneralizedDataset {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|quad| other.contains(quad))
    }
}
impl Eq for GeneralizedDataset {}
