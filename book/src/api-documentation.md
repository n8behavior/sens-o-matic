# API documentation

The server documents its own API. With the server running, which `just book`
does for you (see [Read the docs](development.md#read-the-docs)):

- [Swagger UI](http://localhost:3000/swagger-ui/) is the interactive
  documentation.
- The [OpenAPI document](http://localhost:3000/api-docs/openapi.json) is the
  same description as data.

The code generates both: the annotations of the handlers and the models in
`src/` are their source. `docs/api.yaml` is the original OpenAPI draft, kept as
a file; the document that the server serves is generated, so it is the one to
trust.

The code behind the API is in the
[Rust docs of the crate](rustdoc/sens_o_matic/index.html): the module docs
and the function docs.
