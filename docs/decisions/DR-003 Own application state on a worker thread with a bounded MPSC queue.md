# DR-003: Own application state on a worker thread with a bounded MPSC queue

- Status: Accepted
- Date: 2026-10-04

## Decision

Give each running application one internally started worker thread that owns
its state. Interact with that state through commands sent over a bounded
`crossbeam-channel` queue, used as multiple producers and one consumer (MPSC).
The channel library supports multiple consumers, but the application keeps one
receiver and processes commands sequentially.

`Application::new()` starts the worker immediately and returns an
`ApplicationHandle` containing only a cheaply clonable sender. The default queue
capacity is 128 commands; `with_capacity` permits another bound, including zero
for rendezvous. Keep channel types behind the dispatch module's type aliases.

Send owned requests and return owned results through individual reply channels.
Dispatch waits for queue capacity and returns a pending reply; `try_dispatch`
reports a full queue without losing the call. Slice registration, inspection,
and generated facade methods wait for their replies. Inspections execute on the
same worker and return owned snapshots rather than live state borrows.

Cache static action metadata in `ActionHandle` and generated facades so request
construction can happen locally. Only state access and execution need the
worker. See
[DR-002](DR-002%20Organize%20application%20state%20into%20slices%20and%20typed%20collections.md)
for the storage and item-addressing model.

## Rationale

One owner makes mutable state access explicit and serializes actions without
requiring callers to coordinate locks. Action adapters resolve opaque item
references into temporary Rust borrows on the worker. Application data never
needs to be borrowed across threads, and stored values need `Send` rather than
`Sync`.

A sender provides a small, cloneable interface for callers on different threads.
Commands and replies separate admission from completion: callers can retain a
pending reply instead of blocking until an action finishes. Sequential execution
also lets inspection observe earlier processed changes without racing an action.

Bounding the command queue applies backpressure when producers outrun the
worker. Callers can choose waiting or explicit retry through `try_dispatch`.
Crossbeam supplies bounded blocking channels, rendezvous, nonblocking sends,
and disconnection handling without introducing an async runtime. The choice is
based on those capabilities, not a measured performance advantage over other
channel libraries.

The queue has communication and scheduling costs. We accept them for clear
ownership and a uniform dispatch boundary; direct access under a lock can be
faster for small operations in an uncontended workload.

## Context

Actions must be callable from multiple threads while application state contains
typed arenas and mutable domain objects. Requests must be constructible without
borrowing that state. The engine already has owned requests, process-local item
references, and adapters that resolve collection and item access at execution.

The main alternative is shared state behind `Arc<Mutex<Application>>` or
`Arc<RwLock<Application>>`. Both can support correct dispatch, but callers or
an enclosing API must enforce lock scope, borrowing rules, and access ordering.
Concurrent reads are possible with an `RwLock`, while all mutations still need
exclusive access.

The current application is synchronous and has no requirement for parallel
state mutation, an async executor, durable messaging, or distributed execution.
This record captures the implemented mechanism and its limits.

## Consequences

- Actions and inspections execute one at a time. Concurrent producers have no
  predetermined interleaving; enqueueing order is not a business ordering rule
  between threads. A completed reply confirms that its command ran.
- Queueing, reply-channel allocation, erased requests and results, and thread
  wakeups add overhead compared with direct calls. Throughput is limited by one
  worker; long handlers and inspections delay every subsequent command.
- Keep worker operations short. If substantial computation or I/O is needed,
  consider performing it outside the worker and dispatching a later result,
  with explicit checks for state changes during that interval.
- The bound limits queued command count, not total memory: requests can vary in
  size, blocked producers retain requests, and unconsumed replies retain
  results. Capacity should follow actual workloads rather than being treated as
  a latency or memory guarantee.
- Blocking handle or facade calls from the same application's worker can
  deadlock: it cannot service its own queue while waiting. This includes calls
  from an inspection callback. There is currently no runtime guard or timeout.
- Dropping a pending reply does not cancel an accepted action. Ordinary handler
  errors are returned and leave the worker running; mutations are not rolled
  back.
- On an unwinding panic, the worker discards its potentially inconsistent state
  and drains subsequent commands by dropping them, disconnecting their replies.
  Callers receive errors instead of waiting indefinitely for retained queued
  replies. The worker does not restart or recover state. Abort-mode panics
  cannot be contained by this mechanism.
- Every sender clone, including one retained by a facade, keeps the worker
  alive. Dropping the last sender closes the queue; accepted commands drain
  before normal exit. There is no join handle or explicit shutdown
  acknowledgment, so dropping the last handle does not wait for cleanup to
  finish.
- Worker startup and queue allocation can panic. The design uses one OS thread
  per running application, with the corresponding stack and scheduling costs.
- Snapshots are owned values and may need copying. Inspection provides no
  concurrent read access or long-lived reference to live state.
- The queue is process-local and nondurable. It provides neither persistence nor
  delivery across a process failure. Retrying an action requires
  application-level care because actions are not automatically idempotent.

## Considered alternatives

### Share state through `Arc<RwLock<Application>>`

Rejected for the current API because the primary operation is mutable action
dispatch, which still requires exclusive access. Concurrent reads would help
read-heavy workloads, but add lock lifetime and reader/writer contention
concerns and generally require shared state to be `Sync`. A worker gives a
single explicit execution boundary and owned snapshots. Reconsider shared read
snapshots if measurements show inspection queueing is a bottleneck.

### Share state through `Arc<Mutex<Application>>`

Rejected because it makes execution happen on caller threads and couples
dispatch to lock acquisition and scope. It avoids channel round trips and can be
simpler or faster for small synchronous operations, but provides no
pending-result queue or explicit admission bound. The current design favors
separate state ownership and controllable backpressure.

### Use an unbounded command channel

Rejected because sustained producer overload could accumulate requests without
an admission limit. A bounded queue makes overload visible through waiting or
full-queue errors. It still requires callers to manage request size and retries.

### Use an async task and async channels

Deferred because an executor dependency and async public API are unnecessary for
the current synchronous handlers. Async channels could suit callers already
using an executor; the present blocking methods must not be assumed suitable for
an executor thread without adaptation.

### Use the standard library's bounded channel

Rejected for the current implementation in favor of Crossbeam's channel API and
the project's selected dispatch tooling. `std::sync::mpsc::sync_channel` could
also meet the basic bounded MPSC requirement. No benchmark or correctness claim
establishes it as unsuitable; the external dependency is a maintenance cost.

### Use multiple workers or one worker per slice

Deferred because cross-slice object references and shared application invariants
would need coordination, partitioning, and ordering rules. More workers would
not safely make the same mutable state concurrent by themselves. One owner meets
the current requirements with less complexity.

### Require callers to manage the worker lifecycle explicitly

Rejected for the basic application API because constructing and retaining a
separate dispatcher and worker adds setup for every application. Internal
startup and sender-based lifetime management keep the common path small.
Explicit shutdown and joining remain possible follow-up work if callers need
confirmed cleanup.
