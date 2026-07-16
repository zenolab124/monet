# A Short Reading List on Distributed Consensus

> SYNTHETIC SAMPLE — hand-authored for the streaming golden test. Exercises **reference-style link definitions** (`[text][ref]` usages with trailing `[ref]: url` definitions), which were absent from the real session corpus.

Consensus is the problem of getting a set of unreliable machines to agree on a single value despite crashes and network delays. The classic starting point is the [Paxos protocol][paxos], usually approached through its more teachable cousin, [Raft][raft], whose whole design goal was *understandability*.

## The core papers

Read them roughly in this order:

1. [Raft][raft] — leader election, log replication, and safety, presented so a human can actually implement it.
2. [Paxos Made Simple][paxos-simple] — Lamport's own attempt to demystify the original.
3. The [FLP impossibility result][flp] — why no deterministic protocol can guarantee consensus in a fully asynchronous network with even one faulty process.

The tension between [FLP][flp] and working systems is resolved by relaxing assumptions: real systems assume *partial synchrony*, add randomization, or lean on failure detectors.

## Systems that put it to work

Two production systems are worth studying as case studies:

- [ZooKeeper][zk] uses a Paxos-flavored protocol (Zab) for a coordination service.
- [etcd][etcd] uses [Raft][raft] directly and backs Kubernetes' control plane.

Both expose a small key-value API and lean hard on a replicated log. If you only trace one code path, trace how a write becomes committed once a quorum acknowledges it — that single flow is the whole idea of consensus made concrete.

> A useful mental model: the log *is* the database, and everything else is a cache of the log's replay. Agreement on the log ordering is agreement on reality.

The rendering invariant this fixture guards: reference-style links place their `[ref]: url` definitions far from the `[text][ref]` usage — sometimes hundreds of characters later. A streaming segmenter that cuts between a usage and its definition would, per-segment, render a usage with no resolvable target, diverging from the whole-document render where the definition is in scope.

[paxos]: https://example.com/papers/paxos-part-time-parliament
[paxos-simple]: https://example.com/papers/paxos-made-simple
[raft]: https://example.com/papers/raft-in-search-of-understandable-consensus
[flp]: https://example.com/papers/flp-impossibility
[zk]: https://example.com/systems/zookeeper
[etcd]: https://example.com/systems/etcd
