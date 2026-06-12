# Limit V1 to Single-File Transfers

Production v1 supports one file per Transfer and does not support directory transfer. Directory transfer remains a future capability, but excluding it from v1 avoids archive semantics, empty directories, symlink handling, permissions, path traversal safety, duplicate names, and multi-file resume behavior while the desktop release proves reliable Resumable Transfers.
