# DOCX form sample

A fictional template that follows the items of a Japanese elementary school pupil record (指導要録). All names, the school and the addresses are made up; no real person or school is shown.

- `elementary-school-template.docx`: the template. The plain-text content controls tagged `ibis.form.<ID>` are the input fields (`pupil.name` is in several places).
- `form.json`: the form definition (the field types and the SHA-256 of the template). A field is a type (`"text"`, `"date-ja"`, `"number"`) or an object `{"type": "number", "maxChars": 8, "format": "#,##0"}`; `format` is an Excel number format code, `maxChars` counts the characters shown in the document, and a value over the limit is refused, not cut.
- `record.json`: one record to fill the template with.

```
docx_form fill elementary-school-template.docx form.json record.json out.docx
```

License: the same as ibisOffice (AGPL-3.0-only).
