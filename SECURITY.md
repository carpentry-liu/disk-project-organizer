# Security and data-safety policy

- Scans are read-only.
- Duplicate removal always uses the Windows Recycle Bin.
- The UI refuses to remove every copy in a duplicate group.
- Project organization moves the project root as one unit; it never merges two repositories file-by-file.
- Cross-volume project moves are disabled by default.
- `.git` file worktrees and repositories with multiple worktrees are marked unsafe and skipped.
- System folders, reparse points, junctions, package caches, virtual environments, build folders, and dependency folders are pruned by default.
- Every mutation is recorded in `operations.jsonl`.

Please report security issues privately to the repository owner instead of opening a public issue.

