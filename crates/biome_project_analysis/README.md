# biome_project_analysis

Internal, unpublished issue taxonomy for native project analysis. This partial
slice declares 17 reference categories; every check is **unimplemented** and its
behavior conformance is **untested**. It adds no analysis command or findings.

[`tests/fixtures/compatibility.json`](tests/fixtures/compatibility.json) records
each category's status and pins two separate reference identities:

- [Source commit `07121567e60b73d1e640f395028c525a2b1d9778`](https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/types/issues.ts#L34-L54),
  where `IssueType = keyof Issues` supplies the machine keys. The fixture records
  the immutable source URL and SHA-256 of its response.
- [Published `knip@6.43.0` metadata](https://registry.npmjs.org/knip/6.43.0),
  including the tarball URL and registry-declared integrity. The tarball was not
  downloaded, so its integrity remains independently unverified.

The npm metadata omits `gitHead`; correspondence between the pinned source and
published artifact remains unverified. The inventory does not certify behavior
compatibility or establish that any project can remove Knip.

Run `cargo test -p biome_project_analysis` to check the taxonomy and inventory
offline. These tests do not install or execute Knip.
