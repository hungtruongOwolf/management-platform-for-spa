# @vpyd/api-client

TypeScript types **generated** from the API's OpenAPI document (`apps/api/openapi.json`, written by `pnpm api:spec`) into `src/schema.ts` by `pnpm api:types`; `pnpm api:gen` runs both. Never edit `schema.ts` by hand: regenerate, and let the frontend typecheck catch contract changes. CI diffs both files against the code.

`src/index.ts` holds hand-written short aliases (`Customer = Schemas["CustomerDto"]` …) for the DTOs the web app uses: add one when a screen needs it, remove it when nothing imports it.
