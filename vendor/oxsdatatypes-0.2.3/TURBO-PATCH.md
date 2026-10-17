# gMonthDay leap-day validation

Source: oxsdatatypes 0.2.3, upstream Oxigraph revision
`0e81a29d27575ab69e7f85ea9c7f1058b277fcd0`.

The `gMonthDay` parser validated its day with no reference year, which defaults
to a non-leap year. That rejected `--02-29` before the correctly leap-year-based
value representation was constructed. Validate this partial date using the same
1972 reference year as its value representation. Ordinary `date` validation is
unchanged. Timezone validation is unchanged.

This fixes the datatype parser shared by native RDF terms, SPARQL casts and
SHACL datatype validation. There is no alternate parser or fallback in the
SHACL validator. Regression tests cover leap day with and without timezones,
invalid calendar days and timezone bounds.

See https://www.w3.org/TR/xmlschema11-2/#gMonthDay.
The patch has not been submitted upstream.
