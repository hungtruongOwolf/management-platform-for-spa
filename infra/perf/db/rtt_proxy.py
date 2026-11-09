"""TCP proxy between the API and Postgres that counts round trips and can add latency (db review).

Every time the client (the API) writes after the server last answered, that is one round trip:
a statement, a BEGIN, a COMMIT, or a ping of the connection pool alike. Server-Timing only counts
sqlx statements; this counts everything that waits for the database.

    python -I infra/perf/db/rtt_proxy.py --listen 15432 --target 127.0.0.1:5432 --delay-ms 2.6

Point APP_DATABASE_URL at 127.0.0.1:15432 (TLS off for this local run). Read and reset the
counter with `python -I infra/perf/db/rtt_proxy.py --read 15433` (prints "round_trips=N").
`--delay-ms` delays every answer of the server, to mimic the ~2.6 ms per round trip measured
between Vercel and Supabase (db review §3a).
"""

import argparse
import asyncio
import socket

counter = {"round_trips": 0}


async def pipe(reader, writer, on_chunk, delay):
    try:
        while True:
            data = await reader.read(65536)
            if not data:
                break
            on_chunk()
            if delay:
                await asyncio.sleep(delay)
            writer.write(data)
            await writer.drain()
    except (ConnectionResetError, BrokenPipeError):
        pass
    finally:
        writer.close()


async def handle(client_r, client_w, host, port, delay):
    server_r, server_w = await asyncio.open_connection(host, port)
    state = {"waiting": False}

    def client_sent():
        if not state["waiting"]:
            counter["round_trips"] += 1
            state["waiting"] = True

    def server_sent():
        state["waiting"] = False

    await asyncio.gather(
        pipe(client_r, server_w, client_sent, 0),
        pipe(server_r, client_w, server_sent, delay),
    )


async def control(reader, writer):
    writer.write(f"round_trips={counter['round_trips']}\n".encode())
    counter["round_trips"] = 0
    await writer.drain()
    writer.close()


async def serve(listen, target, control_port, delay_ms):
    host, port = target.rsplit(":", 1)
    delay = delay_ms / 1000.0
    proxy = await asyncio.start_server(
        lambda r, w: handle(r, w, host, int(port), delay), "127.0.0.1", listen
    )
    ctl = await asyncio.start_server(control, "127.0.0.1", control_port)
    async with proxy, ctl:
        await asyncio.gather(proxy.serve_forever(), ctl.serve_forever())


def read(control_port):
    with socket.create_connection(("127.0.0.1", control_port)) as s:
        print(s.recv(100).decode().strip())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--listen", type=int, default=15432)
    parser.add_argument("--target", default="127.0.0.1:5432")
    parser.add_argument("--control", type=int, default=15433)
    parser.add_argument("--delay-ms", type=float, default=0.0)
    parser.add_argument("--read", type=int, help="print and reset the counter on this control port")
    args = parser.parse_args()
    if args.read:
        read(args.read)
    else:
        asyncio.run(serve(args.listen, args.target, args.control, args.delay_ms))


if __name__ == "__main__":
    main()
