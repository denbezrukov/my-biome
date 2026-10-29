//! This is the main binary of Biome.
//!
//! If you're curious about how to use it, check Biome's [website]
//!
//! [website]: https://biomejs.dev

use biome_cli::{
    BiomeCommand, CliDiagnostic, CliSession, biome_command, open_transport, setup_panic_handler,
    to_color_mode,
};
use biome_console::{ConsoleExt, EnvConsole, LogLevel, markup};
use biome_diagnostics::{Diagnostic, PrintDiagnostic, set_bottom_frame};
use biome_fs::OsFileSystem;
use biome_service::workspace;
use std::process::{ExitCode, Termination};
use std::sync::Arc;
use tokio::runtime::Runtime;

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    any(target_os = "macos", target_os = "linux"),
    not(target_env = "musl")
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// Jemallocator does not work on aarch64 with musl, so we'll use the system allocator instead
#[cfg(all(target_env = "musl", target_os = "linux", target_arch = "aarch64"))]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

fn main() -> ExitCode {
    setup_panic_handler();
    set_bottom_frame(main as *const () as usize);

    let mut console = EnvConsole::default();
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let command = match biome_command()
        .fallback_to_usage()
        .run_inner(bpaf::Args::from(arguments.as_slice()))
    {
        Ok(command) => command,
        Err(error) => {
            use bpaf::Parser;
            let analysis_command = bpaf::any::<std::ffi::OsString, _, _>("ARG", Some)
                .many()
                .to_options()
                .command("analyze")
                .to_options()
                .run_inner(bpaf::Args::from(arguments.as_slice()))
                .is_ok();
            let compatibility_profile = bpaf::long("report-profile").argument::<String>("PROFILE");
            let remaining = bpaf::any::<std::ffi::OsString, _, _>("ARG", Some).many();
            let compatibility_profile = bpaf::construct!(compatibility_profile, remaining)
                .to_options()
                .run_inner(bpaf::Args::from(arguments.as_slice()))
                .is_ok_and(|(profile, _)| profile == "knip-6.43");
            error.print_message(100);
            let code = error.exit_code();
            return ExitCode::from(if code != 0 && analysis_command {
                if compatibility_profile { 1 } else { 2 }
            } else {
                code as u8
            });
        }
    };
    let explicit_analysis = matches!(&command, BiomeCommand::Analyze { .. });

    console.set_color(to_color_mode(command.get_color()));

    let is_verbose = command.is_verbose();
    let result = run_workspace(&mut console, command).map_err(|error| {
        if explicit_analysis {
            error.into_project_failure()
        } else {
            error
        }
    });
    match result {
        Err(termination) => {
            if matches!(&termination, CliDiagnostic::ProjectAnalysis(_)) {
                let written = if termination.tags().is_verbose() && is_verbose {
                    console.try_println(
                        LogLevel::Error,
                        markup! {{PrintDiagnostic::verbose(&termination)}},
                    )
                } else {
                    console.try_println(
                        LogLevel::Error,
                        markup! {{PrintDiagnostic::simple(&termination)}},
                    )
                };
                return if written.is_err() {
                    ExitCode::from(2)
                } else {
                    termination.report()
                };
            }
            if termination.tags().is_verbose() && is_verbose {
                console.error(markup! {{PrintDiagnostic::verbose(&termination)}})
            } else {
                console.error(markup! {{PrintDiagnostic::simple(&termination)}})
            }
            termination.report()
        }
        Ok(_) => ExitCode::SUCCESS,
    }
}

fn run_workspace(console: &mut EnvConsole, command: BiomeCommand) -> Result<(), CliDiagnostic> {
    // If the `--use-server` CLI flag is set, try to open a connection to an
    // existing Biome server socket
    let fs = OsFileSystem::default();
    let workspace = (|| {
        if command.should_use_server() {
            let runtime = Runtime::new()?;
            match open_transport(runtime)? {
                Some(transport) => {
                    workspace::client(transport, Box::new(fs)).map_err(CliDiagnostic::from)
                }
                None => Err(CliDiagnostic::server_not_running()),
            }
        } else {
            let threads = command.get_threads();
            Ok(workspace::server(Arc::new(fs), threads))
        }
    })()
    .map_err(|error| command.classify_startup_failure(&OsFileSystem::default(), error))?;

    let session = CliSession::new(&*workspace, console)?;
    session.run(command)
}
