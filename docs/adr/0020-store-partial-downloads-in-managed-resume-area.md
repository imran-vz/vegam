# Store Partial Downloads in a Managed Resume Area

Production v1 writes Receiver-side Partial Downloads to an app-managed resume area, not directly to the user-selected destination. Vegam finalizes or atomically moves the file to the selected destination only after content verification succeeds. Vegam must surface leftover Partial Downloads and their storage use, mark stale or no-longer-resumable entries clearly, and let the user clean them up; it must not silently auto-delete Partial Downloads in v1.
