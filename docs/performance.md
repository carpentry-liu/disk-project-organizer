# Performance design

## Directory traversal

- Windows-native metadata from `dua-core` avoids a separate metadata syscall for every entry.
- Multiple worker threads steal directory jobs dynamically.
- Junctions and symlinks are never followed.
- System, dependency, environment, cache, and build directories are pruned before descent.

## Duplicate files

1. Keep only files above the configured minimum size.
2. Group by exact byte length.
3. Read only the first and last 64 KiB for BLAKE3 quick fingerprints.
4. Calculate full SHA-256 only for surviving candidates.

This keeps the exactness of SHA-256 while reducing disk reads dramatically on ordinary drives.

## UI responsiveness

- Scans run on background threads.
- Results and progress use bounded crossbeam channels.
- Cancellation uses a shared atomic flag.
- The UI renders paged result slices rather than hashing or walking on the UI thread.

## Local release benchmark

Measured on the development workstation with 5,004 generated files:

- Large-file traversal: 37 ms, approximately 132,534 files/second.
- Exact duplicate pipeline: 3,729 ms, approximately 1,341 files/second.
- Project discovery: 232 ms.

These synthetic numbers are an implementation regression baseline, not a guarantee for other disks. HDD/SSD speed, antivirus, file size, directory layout, and cache state materially affect results.
