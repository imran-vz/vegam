# Target the Iroh 1.0 Generation

Vegam will realign on the current Iroh 1.0-generation APIs instead of preserving compatibility with older 0.26-era or 0.95-era code paths. As of 2026-06-16, the latest observed core crate is `iroh = 1.0.0` and the compatible blob crate is `iroh-blobs = 0.103.0`, so implementation should track that line and recheck release status before a production cut, especially because `iroh-blobs = 0.103.0` still carries a production-quality warning.
