use super::*;

impl CompiledShapes {
    /// Expand rules into private additions while borrowing the unchanged source.
    /// The caller owns warning policy and the atomic decision to apply additions.
    pub fn expand_staged(
        &self,
        source: &(impl GraphRead + ?Sized),
        options: Options,
    ) -> Result<RuleExpansion> {
        let mut data = crate::rule_delta::RuleDelta::new(source);
        let recursion_warnings = self.expand_rules_inner(&mut data, options, None)?;
        Ok(RuleExpansion {
            graph: data.into_additions(),
            recursion_warnings,
        })
    }

    /// Validate with private rule additions while borrowing the unchanged base.
    /// Neither pending caller writes nor committed data are modified.
    pub fn validate_staged(
        &self,
        source: &(impl GraphRead + ?Sized),
        options: Options,
    ) -> Result<ValidationReport> {
        Ok(self.validate_delta(source, options)?.report)
    }

    /// Validate privately and return the rule delta for an atomic caller-owned commit.
    pub fn validate_delta(
        &self,
        source: &(impl GraphRead + ?Sized),
        options: Options,
    ) -> Result<ValidationOutcome> {
        let mut data = crate::rule_delta::RuleDelta::new(source);
        let warnings = self.expand_rules_inner(&mut data, options.clone(), None)?;
        let report = self.validate_expanded(&data, options, warnings)?;
        Ok(ValidationOutcome {
            report,
            additions: data.into_additions(),
        })
    }

    /// Validate and expand directly on a caller-owned transaction.
    /// Pending writes are visible. The caller must abort on an error and owns
    /// the decision to commit a conforming report; this method never commits.
    pub fn validate_transaction(
        &self,
        transaction: &mut oxigraph::store::Transaction<'_>,
        options: Options,
    ) -> Result<ValidationReport> {
        let warnings = self.expand_rules_inner(transaction, options.clone(), None)?;
        self.validate_expanded(&*transaction, options, warnings)
    }

    /// Validate a consistent Store transaction without copying its base data.
    /// Commit rule additions only when inplace is requested and execution succeeds.
    pub fn validate_store(
        &self,
        store: &oxigraph::store::Store,
        options: Options,
        inplace: bool,
    ) -> Result<ValidationReport> {
        let mut transaction = store
            .start_transaction()
            .map_err(|error| Error::Runtime(error.to_string()))?;
        let report = {
            let mut source = crate::rdf11_store::Rdf11Transaction::new(&mut transaction);
            let warnings = self.expand_rules_inner(&mut source, options.clone(), None)?;
            self.validate_expanded(&source, options, warnings)?
        };
        if inplace {
            transaction
                .commit()
                .map_err(|error| Error::Runtime(error.to_string()))?;
        }
        Ok(report)
    }

    /// Expand rules directly on a Store transaction, returning only additions.
    /// With inplace=false, the transaction is aborted after producing its delta.
    pub fn expand_store(
        &self,
        store: &oxigraph::store::Store,
        options: Options,
        inplace: bool,
    ) -> Result<RuleExpansion> {
        let mut transaction = store
            .start_transaction()
            .map_err(|error| Error::Runtime(error.to_string()))?;
        let mut graph = Dataset::new();
        let recursion_warnings = {
            let mut source = crate::rdf11_store::Rdf11Transaction::new(&mut transaction);
            self.expand_rules_inner(&mut source, options, Some(&mut graph))?
        };
        if inplace {
            transaction
                .commit()
                .map_err(|error| Error::Runtime(error.to_string()))?;
        }
        Ok(RuleExpansion {
            graph,
            recursion_warnings,
        })
    }
}
