# Memory diagnostics

How to tell whether a torii process that keeps growing is leaking, and where.

## What grows and what does not

The database file is not loaded into RAM. SQLite's page cache is capped process-wide by
`--sql.soft_memory_limit` (1 GB by default) and the mmap window by `--sql.mmap_size` (256 MB).
A larger database needs more disk, not more memory. A fresh process on a large database that
settles well under 1 GB confirms this.

Memory that scales with uptime rather than with data comes from in-process state:

- **Subscriptions.** Every gRPC subscription holds a channel buffered to
  `--grpc.subscription_buffer_size` messages. Every GraphQL subscription holds an unbounded one.
  A client that vanished without closing its socket keeps both alive until the connection is
  detected as dead.
- **Allocator retention.** torii runs on jemalloc, which keeps freed pages around and returns
  them to the OS slowly. Resident memory is a high-water mark, so a flat line that never comes
  down is normal. A line that keeps climbing is not.
- **Caches without eviction.** The contract class cache and the token id registry grow with the
  number of distinct contracts and tokens ever indexed.

## What this build adds

Dead-peer detection on the public listener. The `--grpc.tcp_keepalive_interval`,
`--grpc.http2_keepalive_interval` and `--grpc.http2_keepalive_timeout` options now also apply
to the HTTP port that clients actually connect to. Previously they covered only the internal
hop, and an idle dead client relied on the OS default of two hours before the first probe.

A sweep every 30 seconds that drops gRPC subscribers whose client has hung up, instead of
waiting for the next update to discover them.

Gauges, exported on the metrics endpoint (`--metrics`) and refreshed every 15 to 30 seconds:

| Metric | Meaning |
|---|---|
| `torii_memory_jemalloc_allocated_bytes` | Bytes the application currently holds. **The leak signal.** |
| `torii_memory_jemalloc_resident_bytes` | Bytes jemalloc keeps in RAM, including freed pages not yet returned. This is what RSS shows. |
| `torii_memory_jemalloc_retained_bytes` | Address space jemalloc keeps after use. Large values are retention, not a leak. |
| `torii_memory_resident_bytes` | RSS from the kernel (Linux only). |
| `torii_proxy_connections` | Client connections open on the public listener right now. |
| `torii_proxy_connections_total` | Connections accepted since start. |
| `torii_grpc_subscribers{kind}` | Registered gRPC subscribers per kind (entity, token_balance, ...). |
| `torii_grpc_subscribers_dropped_total{kind,reason}` | Subscribers removed: `full` (slow client), `closed` (hung up), `pruned` (found by the sweep). |
| `torii_broker_subscribers{kind}` | Streams on the in-memory broker. One per kind is the gRPC dispatcher; anything above that is a GraphQL subscription. |

The upstream `jemalloc_*` gauges from `dojo-metrics` accumulate across scrapes and are not
usable for this. Graph the `torii_memory_*` ones.

Log lines, independent of `--metrics`:

- `torii::runner::memory` logs a memory summary at `info` every five minutes.
- `torii::grpc::server::subscriptions::monitor` logs at `info` whenever subscriber counts change
  or the sweep pruned something, and at `debug` otherwise.
- `torii::server::proxy` logs each connection open and close at `debug`.

## Reading the numbers

1. **Allocated flat, resident climbing.** Allocator retention. Return memory faster with
   `_RJEM_MALLOC_CONF=background_thread:true,dirty_decay_ms:5000,muzzy_decay_ms:5000`.
2. **Allocated climbing, connections and subscribers climbing with it.** Zombie clients. Check
   that keepalives are enabled and shorten them. Lower `--grpc.subscription_buffer_size`; the
   default of 16384 messages per subscriber is far more than a healthy client needs.
3. **Allocated climbing, broker streams above one per kind with no GraphQL clients.** Leaked
   GraphQL subscriptions. These are unbounded; consider not exposing GraphQL subscriptions.
4. **Allocated climbing, connections and subscribers flat.** Look at the caches. Correlate with
   the number of distinct contracts and tokens indexed.

## Reproducing a zombie client

Open a few subscriptions from a client, then suspend the client process (`kill -STOP <pid>`) so
the socket stays open but nothing is read. Watch `torii_grpc_subscribers` stay put and
`torii_memory_jemalloc_allocated_bytes` grow as updates arrive. Resume or kill the client and
confirm the subscriber count drops within the keepalive window and allocated bytes follow.
