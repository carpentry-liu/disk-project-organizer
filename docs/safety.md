# Safety model

| Operation | Default behavior |
|---|---|
| Large-file scan | Read-only |
| Duplicate scan | Read-only |
| Duplicate removal | Explicit selection, confirmation, Recycle Bin |
| Project scan | Read-only |
| Project organization | Preview first; safe projects only |
| Cross-drive move | Disabled |
| Git worktree | Refused and marked unsafe |
| Existing destination | Refused |
| Reparse point | Refused |
| Git verification failure | Reported immediately in audit log |

The application never performs recursive deletion of a project and never merges the contents of two Git repositories.

