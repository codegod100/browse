# radicle-job (vendored, read-only)

Read-only subset of [`radicle-job`](https://crates.io/crates/radicle-job) 0.6.0, adapted for `radicle` 0.25.

Browse only needs to list and inspect `xyz.radworks.job` COBs (`Jobs::open_readonly`), so write paths (`JobMut`, `create`, …) are omitted until upstream publishes a 0.25-compatible release.
