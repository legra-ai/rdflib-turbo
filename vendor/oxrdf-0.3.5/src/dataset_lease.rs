//! Owner-checked leases on the existing immutable records, not a new RDF index.
use super::{Dataset,indexes::QuadRecord};
use crate::QuadRef;
use std::fmt;
use std::hash::{Hash,Hasher};
use std::sync::{Arc,Weak};

/// A moved mutable dataset plus one call-local ownership token.
pub struct DatasetLeaseBuilder{data:Dataset,origin:Arc<()>}
impl Default for DatasetLeaseBuilder{fn default()->Self{Self::new()}}
impl DatasetLeaseBuilder{
    /// Create one empty RDF owner.
    pub fn new()->Self{Self::from_dataset(Dataset::new())}
    /// Move an existing RDF owner without cloning its relation or dictionary.
    pub fn from_dataset(data:Dataset)->Self{Self{data,origin:Arc::new(())}}
    /// Insert through the existing indexes and return whether the fact was new.
    pub fn insert<'a>(&mut self,q:impl Into<QuadRef<'a>>)->bool{self.data.insert(q)}
    /// Lease an actual already inserted record; this unbound lease is not hashable.
    pub fn quad_lease(&self,q:QuadRef<'_>)->Option<InsertionQuadLease>{Some(InsertionQuadLease{record:record(&self.data,q)?,origin:Arc::downgrade(&self.origin)})}
    /// Freeze this same moved owner and token. No data, key or dictionary clone.
    pub fn freeze(self)->FrozenDataset{FrozenDataset{data:self.data,origin:self.origin}}
}
/// A pre-freeze record and provenance; bind before using it as metadata identity.
#[derive(Clone,Debug)]
pub struct InsertionQuadLease{record:Arc<QuadRecord>,origin:Weak<()>}
/// The immutable call-local RDF owner and its private provenance token.
pub struct FrozenDataset{data:Dataset,origin:Arc<()>}
impl FrozenDataset{
    /// Borrow the actual dataset and dictionary.
    pub fn dataset(&self)->&Dataset{&self.data}
    fn record(&self,q:QuadRef<'_>)->Option<Arc<QuadRecord>>{record(&self.data,q)}
    /// Check an insertion lease before hashing or map insertion.
    pub fn bind_lease(&self,lease:InsertionQuadLease)->Result<DatasetQuadLease,DatasetLeaseError>{
        if !Weak::ptr_eq(&lease.origin,&Arc::downgrade(&self.origin)){return Err(DatasetLeaseError::ForeignOwner);}
        if !self.data.gspo.contains(lease.record.as_ref()){return Err(DatasetLeaseError::MissingRecord);}
        Ok(DatasetQuadLease{record:lease.record,origin:lease.origin})
    }
    /// Obtain a checked identity for an actual fact of this owner.
    pub fn lease_quad(&self,q:QuadRef<'_>)->Option<DatasetQuadLease>{Some(DatasetQuadLease{record:self.record(q)?,origin:Arc::downgrade(&self.origin)})}
    /// Reject foreign provenance before consulting an assessment map.
    pub fn validate_lease(&self,lease:&DatasetQuadLease)->Result<(),DatasetLeaseError>{
        if !Weak::ptr_eq(&lease.origin,&Arc::downgrade(&self.origin)){return Err(DatasetLeaseError::ForeignOwner);}
        if !self.data.gspo.contains(lease.record.as_ref()){return Err(DatasetLeaseError::MissingRecord);}Ok(())
    }
}
impl std::ops::Deref for FrozenDataset{type Target=Dataset;fn deref(&self)->&Dataset{&self.data}}
/// Checked owner-local storage key. RDF identity is the complete encoded record.
#[derive(Clone,Debug)]
pub struct DatasetQuadLease{record:Arc<QuadRecord>,origin:Weak<()>}
impl PartialEq for DatasetQuadLease{fn eq(&self,other:&Self)->bool{Weak::ptr_eq(&self.origin,&other.origin)&&self.record==other.record}}
impl Eq for DatasetQuadLease{}
impl Hash for DatasetQuadLease{fn hash<H:Hasher>(&self,state:&mut H){self.record.hash(state);}}
/// A lease does not belong to the requested frozen owner, or its fact is absent.
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum DatasetLeaseError{
    /// Ownership provenance differs, even if a cloned dictionary uses equal IDs.
    ForeignOwner,
    /// The leased record is not an actual fact of the owner.
    MissingRecord,
}
impl fmt::Display for DatasetLeaseError{fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{match self{Self::ForeignOwner=>f.write_str("Foreign Dataset record lease"),Self::MissingRecord=>f.write_str("Dataset record lease has no stored fact")}}}
impl std::error::Error for DatasetLeaseError{}
fn record(data:&Dataset,q:QuadRef<'_>)->Option<Arc<QuadRecord>>{let key=QuadRecord::from_spog(data.encoded_quad(q)?);Some(Arc::clone(&data.gspo.get(&key)?.record))}

#[cfg(test)]
#[path="dataset_lease_tests.rs"]
mod tests;
