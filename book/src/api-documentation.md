# API documentation

The server documents its own API. With the server running (see
[Getting started](getting-started.md)):

- Swagger UI, the interactive documentation, is at
  `http://localhost:3000/swagger-ui/`.
- The OpenAPI document is at `http://localhost:3000/api-docs/openapi.json`.

The code generates both: the annotations of the handlers and the models in
`src/` are their source. `docs/api.yaml` is the original OpenAPI draft, kept as
a file; the document that the server serves is generated, so it is the one to
trust.
