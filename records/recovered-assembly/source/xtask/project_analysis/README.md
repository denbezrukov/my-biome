# Project analysis conformance

This internal harness pins Knip 6.43.0 as a development oracle. The native Rust
engine implements partial file reachability, tested through owned snapshots and
service source acquisition. Other issue categories and CLI integration remain
unsupported. The shared conformance cases and compatibility profile remain
unvalidated; deferred plugins and executable extensions are unsupported. An
oracle observation is not a native conformance pass.

Run the three reviewed smoke cases from the repository root:

```sh
cargo run -p xtask_project_analysis -- conformance --case orphan-file --oracle-only
cargo run -p xtask_project_analysis -- conformance --case namespace-known-key --oracle-only
cargo run -p xtask_project_analysis -- conformance --case production-defaults --oracle-only
```

The harness verifies downloaded archive bytes against the pinned SHA-512 before
installing anything. It verifies the complete dependency-lock SHA-256 and exact
Node version from `oracle/identity.json`, then runs `npm ci --ignore-scripts` in
`oracle/`. Node must satisfy `^20.19.0 || >=22.12.0`; the recorded executable uses
v24.19.0. Installations never target the fixture or a consuming project. `curl`,
Node and npm are needed only for explicit oracle runs, not for Rust unit tests.
All transitive versions and integrity values are in `oracle/package-lock.json`.
The npm registry archive and source-research Git commit are separate pins;
matching release names alone do not establish equivalent source bytes.

`--artifact <archive>` verifies a local archive instead of downloading it. The
retained corruption observation used an altered byte of the real archive and
Node/npm sentinels, showing rejection before installation or execution.

`observations/` retains the actual executable and Node paths and digests, command,
working directory, environment overrides, installation output, fixture digest,
raw stdout/stderr/exit code and normalized output. Expected oracle outputs are
reviewed separately in `expectations/oracle/`. Differences are retained as ignored
`*.pending.json` files and fail the command; there is no automatic acceptance.
`oracle/validation.json` becomes validated only after all three reviewed cases
pass with the same identity and current fixtures/expectations. This validates
only the `oracle-smoke-3` profile, never native parity or all Knip capabilities.

These fixtures are authored for this harness:

- `orphan-file` reports the unreachable file and protects the entry file.
- `namespace-known-key` reports `ns.unused` after a known property access even
  though the whole object is also used; a second opaque namespace stays protected.
- `production-defaults` uses default entry/project patterns, protects entry
  exports, finds the orphan, unused production dependency and unused value export,
  and omits development dependencies and both catalog categories.

The smoke normalizer preserves names, namespace details, positions and empty
issue categories. Its severity is `error` because these fixtures use default
rules and do not enable cycles. The Knip JSON reporter does not expose coverage,
capabilities or graph evidence; those fields are explicitly unavailable, not
inferred from a zero-issue report. Native result normalization separately retains
snapshot text, all semantic digests, scope, evidence and coverage and remaps local
IDs. It must not be used for stale-fix revision checks.

The planned native invocation is:

```sh
cargo run -p xtask_project_analysis -- conformance --case orphan-file --engine /path/to/engine
```

The binary receives `--case <name>` in the fixture directory and must emit its
own JSON. Its output is retained separately and the harness exits untested until
reviewed native expectations and capability support exist. Oracle output is never
substituted for engine output. `crates/biome_project_analysis/tests/support` loads
owned snapshots and executes the partial engine for native reachability fixtures.
Service tests additionally exercise real parsing, resolution and snapshot acquisition.
These native tests do not promote the separate conformance harness or its profile.

`compatibility.json` enumerates issue keys, root/workspace configuration, CLI
flags (including short aliases), reporters, fix families and all 198 deferred
plugins with explicit owner/status. Plugin keys apply at both configuration
scopes. The separate `static_schema` field records declarative JSON/JSONC parsing
support; it does not promote native analysis status or establish migration eligibility.
The configuration tests exhaustively check these static root/workspace keys and reject
all deferred plugin keys. Native passed/failed cases and unsupported cases have separate lists.
`reference/inventory.md` and `reference/sources.json` preserve the original source
research and hashes. That research is not execution evidence. `reference/tests.json`
lists all 828 pinned test paths, owners and dispositions, with selected exact test
titles and negative assertions. Unported cases are not counted as semantic
coverage. `reference/additional-sources.json` records the additional pinned bytes
used for this inventory. The upstream ISC notice is retained in
`reference/LICENSE.knip` for quoted test excerpts.
