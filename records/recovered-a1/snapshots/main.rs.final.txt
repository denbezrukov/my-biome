mod artifact;
mod conformance;

use anyhow::{Context, bail, ensure};
use std::{env, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1);
    ensure!(
        args.next().as_deref() == Some("conformance"),
        "usage: conformance --case <name> (--oracle-only | --engine <binary>) [--artifact <archive>]"
    );
    let mut case = None;
    let mut oracle_only = false;
    let mut engine = None;
    let mut artifact = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--case" => case = Some(args.next().context("--case needs a name")?),
            "--oracle-only" => oracle_only = true,
            "--engine" => {
                engine = Some(PathBuf::from(
                    args.next().context("--engine needs a binary")?,
                ))
            }
            "--artifact" => {
                artifact = Some(PathBuf::from(
                    args.next().context("--artifact needs a path")?,
                ))
            }
            _ => bail!("unknown argument: {arg}"),
        }
    }
    conformance::run(
        &case.context("--case is required")?,
        oracle_only,
        engine.as_deref(),
        artifact.as_deref(),
    )
}
