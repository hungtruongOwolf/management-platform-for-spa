"""Signed-in requests against a LOCAL API, with the database behind rtt_proxy.py (db review).

For each endpoint, N sequential samples: the API's own Server-Timing (sqlx statements, db time,
server total) and the round trips the proxy saw while the request ran (every statement, BEGIN,
COMMIT and pool ping, including the ping sqlx sends in the background when a connection goes back
to the pool).

    PERF_PASSWORD=… python -I infra/perf/db/local_requests.py http://127.0.0.1:8080 0905000002 [samples]

The password comes from the environment only (never argv). Local only: plain HTTP.
"""

import datetime
import http.client
import json
import os
import re
import socket
import statistics
import sys
import time
import urllib.parse

CONTROL_PORT = int(os.environ.get("RTT_CONTROL_PORT", "15433"))


def round_trips():
    with socket.create_connection(("127.0.0.1", CONTROL_PORT)) as s:
        line = s.recv(100).decode().strip()
    parts = dict(kv.split("=", 1) for kv in line.split())
    return int(parts["round_trips"]), int(parts["pings"])


def main():
    base = urllib.parse.urlparse(sys.argv[1])
    phone = sys.argv[2]
    samples = int(sys.argv[3]) if len(sys.argv) > 3 else 6
    password = os.environ["PERF_PASSWORD"]
    origin = f"{base.scheme}://{base.netloc}"
    conn = http.client.HTTPConnection(base.hostname, base.port or 80, timeout=30)

    def request(method, path, body=None, cookie=None):
        headers = {"Origin": os.environ.get("PERF_ORIGIN", "http://localhost:5173")}
        if body is not None:
            headers["Content-Type"] = "application/json"
        if cookie:
            headers["Cookie"] = cookie
        conn.request(method, path, json.dumps(body) if body is not None else None, headers)
        r = conn.getresponse()
        data = r.read()
        return r, data

    r, _ = request("POST", "/api/v1/auth/login", {"phone": phone, "password": password})
    if r.status != 200:
        raise SystemExit(f"login answered {r.status}")
    cookie = r.getheader("Set-Cookie").split(";", 1)[0]
    today = datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=7))).date()
    _, body = request("GET", "/api/v1/customers", cookie=cookie)
    customers = json.loads(body)
    items = customers.get("items", customers) if isinstance(customers, dict) else customers
    customer = items[0]["id"] if items else None
    endpoints = [
        ("me", "/api/v1/me"),
        ("staff", "/api/v1/staff"),
        ("visits day", f"/api/v1/visits?date={today}"),
        ("appointments day", f"/api/v1/appointments?from={today}&to={today}"),
        ("attendance month", f"/api/v1/attendance?month={today:%Y-%m}"),
        ("reports daily", f"/api/v1/reports/daily?date={today}"),
    ]
    if customer:
        endpoints.append(("customer history", f"/api/v1/customers/{customer}/history"))

    rows = []
    for name, path in endpoints:
        rts, pings, statements, db, total = [], [], [], [], []
        for _ in range(samples):
            time.sleep(0.4)  # background release pings of the previous request finish
            round_trips()  # reset
            r, _ = request("GET", path, cookie=cookie)
            time.sleep(0.3)
            rt, ping = round_trips()
            rts.append(rt)
            pings.append(ping)
            timing = r.getheader("Server-Timing") or ""
            m = re.search(r'db;dur=([\d.]+);desc="(\d+) statements", total;dur=([\d.]+)', timing)
            if m:
                db.append(float(m.group(1)))
                statements.append(int(m.group(2)))
                total.append(float(m.group(3)))
        rows.append({
            "endpoint": name,
            "status": r.status,
            "round_trips_p50": statistics.median(rts),
            "of_which_pings_p50": statistics.median(pings),
            "statements": statistics.median(statements) if statements else None,
            "db_ms_p50": round(statistics.median(db), 1) if db else None,
            "total_ms_p50": round(statistics.median(total), 1) if total else None,
        })
    request("POST", "/api/v1/auth/logout", {}, cookie=cookie)
    print(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
