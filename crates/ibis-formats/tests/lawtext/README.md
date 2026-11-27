# e-Gov fixture

Source: [e-Gov 法令検索（デジタル庁）法令 API](https://laws.e-gov.go.jp/api/1/lawdata/321CONSTITUTION), retrieved 2026-11-18. The fixture contains the original Law element extracted from the API response, without the response wrapper. [Source terms](https://laws.e-gov.go.jp/terms).

SHA-256: `c1085ee027eccaa6ac956c5877ad3a169288775d82a3f473e67b2503f8a920f6`.

CI uses this snapshot without contacting the API. Lawtext is generated from the XML by ibisOffice and parsed back in the test. This verifies conversion with a real source document. It does not independently validate compatibility with other Lawtext implementations.

`administrative-procedure-act.lawtext` is the Administrative Procedure Act excerpt from the [upstream Lawtext README](https://github.com/yamachig/Lawtext/blob/60567c6ca7de6b14c8c1d895338c4614b49969d1/README.md#lawtextの例--example-of-lawtext), with its original whitespace and a final newline. The text is a public Japanese statute.
