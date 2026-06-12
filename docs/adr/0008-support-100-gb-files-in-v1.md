# Support 100 GB Files in V1

Production v1 must support single-file Transfers of at least 100 GB. This sets the engineering bar for bounded memory usage, streaming file access, persistent partial downloads, and resumability; implementations that read an entire file into memory or require restarting large Transfers after ordinary interruptions do not meet the v1 production requirement.
