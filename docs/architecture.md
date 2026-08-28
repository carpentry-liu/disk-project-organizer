# Architecture

```text
egui UI
  ├─ background job controller + cancellation
  ├─ large-file scanner
  ├─ duplicate scanner
  │    ├─ size grouping
  │    ├─ BLAKE3 quick fingerprint
  │    └─ SHA-256 exact verification
  ├─ project detector
  │    ├─ project markers
  │    ├─ language and description inference
  │    └─ Git/worktree inspection
  └─ operations
       ├─ Recycle Bin
       ├─ whole-project move
       ├─ Git state verification
       └─ JSONL audit log
```

Filesystem enumeration is provided by `dua-core`, which uses a work-stealing pool and native Windows metadata. Expensive content hashing is limited to same-size candidates and parallelized with Rayon.

