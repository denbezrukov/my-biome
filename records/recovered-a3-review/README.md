# A3 reviewer source recovery

These files transcribe exact source text still visible in the original review history at initial A3 commit `52708197fbcac4abe8231e6c36bda61db6ff4ce2`.

- `inventory.rs`: nine noncontiguous fragments, 435 original lines total. Each filename and the manifest identifies its original line range. Fragments may end inside an expression.
- `paths.rs`: complete 101-line initial-head file.
- `tsconfig.rs`: two fragments, 52 original lines total.
- `inventory.tests.rs`: no exact source remains; its original full read was compacted into prose.
- Full `inventory.rs`: its original full read was also compacted; no missing spans are reconstructed here.
- Final fix commit `af4b6125c3db4cb82953be7d2e4440c398b39add`: no source recovered by this reviewer.

These are initial-head artifacts, not final-fix files. The parent must assemble source separately with other retained evidence. The manifest records original paths, stage, completeness, and artifact SHA-256 values. Hashes establish recovered artifact integrity; they are not independently retained original-file hashes.

Only source excerpts and recovery metadata were saved. No review or project code was rerun; no main-repository edits were made.
