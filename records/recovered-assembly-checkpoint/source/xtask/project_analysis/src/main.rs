mod artifact;
mod compare;
mod conformance;
mod execution;
mod normalize;

use anyhow::{Context, bail, ensure};
use std::{env, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next();
    if command.as_deref() == Some("observe") {
        let request = args
            .next()
            .context("observe requires request and output paths")?;
        let output = args.next().context("observe requires an output path")?;
        ensure!(args.next().is_none(), "unexpected observe argument");
        let request = serde_json::from_slice(&std::fs::read(request)?)?;
        let observation = execution::run_bounded(request)?;
        std::fs::write(output, serde_json::to_vec_pretty(&observation)?)?;
        return Ok(());
    }
    if command.as_deref() == Some("normalize") {
        let format = args
            .next()
            .context("normalize requires format, observation and output paths")?;
        let input = args
            .next()
            .context("normalize requires an observation path")?;
        let output = args.next().context("normalize requires an output path")?;
        let companion = args
            .next()
            .map(std::fs::read)
            .transpose()?
            .map(|bytes| serde_json::from_slice(&bytes))
            .transpose()?;
        ensure!(args.next().is_none(), "unexpected normalize argument");
        let observation = serde_json::from_slice(&std::fs::read(input)?)?;
        let normalized = normalize::report(&format, &observation, companion)?;
        std::fs::write(output, serde_json::to_vec_pretty(&normalized)?)?;
        return Ok(());
    }
    if command.as_deref() == Some("compare") {
        let expected = args
            .next()
            .context("compare requires expected and actual paths")?;
        let actual = args.next().context("compare requires an actual path")?;
        ensure!(args.next().is_none(), "unexpected compare argument");
        let expected = serde_json::from_slice(&std::fs::read(expected)?)?;
        let actual = serde_json::from_slice(&std::fs::read(actual)?)?;
        ensure!(
            compare::reports_match(&expected, &actual),
            "report observations differ"
        );
        return Ok(());
    }
    ensure!(
        command.as_deref() == Some("conformance"),
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
