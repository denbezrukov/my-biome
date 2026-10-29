# Retained B1 fix1 review source recovery

Historical stage: final B1 fix1, HEAD df22208cc013659d537dc12e6d95cbf3c25ba11c.
Reviewed range: a62fb4875d3d9aca9fc3ffcc74b22b56913b9aed..df22208cc013659d537dc12e6d95cbf3c25ba11c.

Source is transcribed exclusively from literal tool output retained in this review conversation. No current rebuilt checkout was read, no implementation was inferred, and no project code was executed for recovery.

Exact complete retained source: crates/biome_service/src/project_analysis/reachability.tests.rs (full-file cat output). Historical SHA-256: c908424f0f2a96b50caa26d4692ed91eb04339cd6f7510474f3efa8fbc2b5f27.

Collector historical SHA-256: 54682db138efd0a96319e43d631bc6ce6a36c6430d53ba84a7716b487fb69588. Full-file tool output was truncated, but overlapping literal excerpts survive. Fragments are saved with explicit original line ranges; they are not a claim of a recovered complete file.

Engine crates/biome_project_analysis/src/reachability.rs historical SHA-256: 32cfe52e3ac3a4349b41777ec6269435d12f9e154d2d93a100cfe7fe8c5393a2. Only bounded excerpts survive, not the full file.

Service owned_facts.rs historical SHA-256: 5a6a629058fe7fcc2e2e3d7eff84beadd70ffb9b26e19315ee5f0059a29d46b4. Only bounded excerpts survive.

Service snapshot.rs historical SHA-256: 121fe30d067edeb5a9beb5a38283e1c31d884a51133439367ac82a25d920bfec. No source literals were read in this review; exact snapshot integration source is unavailable here.

No contents of the six engine JSON fixtures were read in this review. Their paths and historical hashes survive in historical-source-hashes.txt, but their literal contents cannot be recovered from this conversation.

This is a source-recovery export, not a new review, implementation, or claim that the entire historical B1 state has been recovered.

## Export verification

The exported complete service test file was hashed directly after transcription. Its SHA-256 is c908424f0f2a96b50caa26d4692ed91eb04339cd6f7510474f3efa8fbc2b5f27, exactly matching the retained historical final hash. It contains 821 lines. Only recovery-export files were read for this check.

The collector fragments cover original lines 1–647, 648–968, and 972–EOF (907 lines in the final fragment, ending at original line 1878). Original lines 969–971 are the only missing collector lines; they were not inferred or supplied. Concatenating fragments without restoring the gap does not produce the complete historical file. Smaller overlapping collector fragments retain the changed loader branch and helper separately.

final-collector-fix1.diff retains the exact supplied collector U10 diff for a62fb4875d3d9aca9fc3ffcc74b22b56913b9aed..df22208cc013659d537dc12e6d95cbf3c25ba11c.

Additional literal fragments:

- crates/biome_project_analysis/src/reachability.rs: 33–129, 134–189, 190–244, 268–306, 307–370, 373–490.
- crates/biome_project_analysis/src/lib.rs: 27–85 (ends inside the analysis scope expression; no continuation retained).
- crates/biome_service/src/project_analysis/owned_facts.rs: 154–172 and 372–416.

Fragment line counts were checked against their labels. No fragment is advertised as a complete source file. No fixture data, missing closures, missing imports, or omitted scope implementation was synthesized.
