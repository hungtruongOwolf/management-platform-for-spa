// Measurements from real probes in Vietnam (perf page §2a) through the public Globalping API.
// Never sends cookies to the third party: public endpoints only.
//
//   node infra/perf/globalping.mjs http <staging-host> /api/v1/health [country=VN] [probes=3]
//   node infra/perf/globalping.mjs ping ec2.ap-southeast-1.amazonaws.com
const [type, target, requestPath = "/", country = "VN", limit = "3"] = process.argv.slice(2);
if (!type || !target) {
  console.error("usage: globalping.mjs <http|ping> <host> [path] [country] [probes]");
  process.exit(2);
}
const measurementOptions =
  type === "http" ? { protocol: "HTTPS", request: { path: requestPath, method: "GET" } } : { packets: 8 };
const created = await fetch("https://api.globalping.io/v1/measurements", {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ type, target, locations: [{ country, limit: Number(limit) }], measurementOptions }),
});
const { id, ...error } = await created.json();
if (!id) {
  console.error(JSON.stringify(error));
  process.exit(1);
}
let measurement;
for (let i = 0; i < 30; i++) {
  await new Promise((r) => setTimeout(r, 1500));
  measurement = await (await fetch(`https://api.globalping.io/v1/measurements/${id}`)).json();
  if (measurement.status !== "in-progress") break;
}
for (const { probe, result } of measurement.results) {
  if (type === "http")
    console.log(probe.city, probe.network, result.statusCode, JSON.stringify(result.timings), result.headers?.["x-vercel-id"] ?? "");
  else console.log(probe.city, probe.network, JSON.stringify(result.stats));
}
