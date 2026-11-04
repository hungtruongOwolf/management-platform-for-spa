import { useQuery } from "@tanstack/react-query";

interface Health {
  status: string;
  version: string;
}

function isHealth(value: unknown): value is Health {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as Record<string, unknown>)["status"] === "string" &&
    typeof (value as Record<string, unknown>)["version"] === "string"
  );
}

async function fetchHealth(): Promise<Health> {
  const response = await fetch("/api/v1/health", { credentials: "same-origin" });
  if (!response.ok) throw new Error(`health check failed: ${response.status}`);
  const body: unknown = await response.json();
  if (!isHealth(body) || body.status !== "ok") throw new Error("unexpected health payload");
  return body;
}

/** Smoke indicator proving the SPA → API path (dev proxy / Caddy) works end to end. */
export function ApiStatus() {
  const health = useQuery({ queryKey: ["health"], queryFn: fetchHealth });

  if (health.isPending) return <p role="status">API: đang kiểm tra…</p>;
  if (health.isError) {
    return (
      <p role="status" className="text-danger">
        API: không kết nối được
      </p>
    );
  }
  return (
    <p role="status" className="text-success">
      API: đã kết nối (v{health.data.version})
    </p>
  );
}
