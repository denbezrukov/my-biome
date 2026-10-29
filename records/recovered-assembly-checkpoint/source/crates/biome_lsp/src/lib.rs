#![deny(clippy::use_self)]

mod capabilities;
mod diagnostics;
mod documents;
mod extension_settings;
mod handlers;
mod project_analysis;
mod requests;
mod server;
#[cfg(test)]
mod server_test_utils;
mod session;
mod utils;

#[cfg(test)]
#[path = "project_analysis.tests.rs"]
mod project_analysis_tests;

pub use crate::extension_settings::WorkspaceSettings;
pub use crate::server::{LSPServer, ServerConnection, ServerFactory};
