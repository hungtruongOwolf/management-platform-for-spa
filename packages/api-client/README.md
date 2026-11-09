# @vpyd/api-client

TypeScript types **generated** from the API contract `apps/api/openapi.json` into `src/schema.ts` (`pnpm api:gen` = `api:spec` from the Rust code, then `api:types` with openapi-typescript). Never edit `schema.ts` by hand; `src/index.ts` only re-exports named aliases. CI regenerates both files and fails on any diff.
