use crate::server_test_utils::*;
use anyhow::{Context, Result};
use biome_fs::MemoryFileSystem;
use futures::channel::mpsc::channel;
use futures::{SinkExt, StreamExt};
use serde_json::{from_value, json};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tower::Service;
use tower_lsp_server::jsonrpc::{Request, Response};
use tower_lsp_server::ls_types::{self as lsp, TextEdit};

#[tokio::test]
async fn first_document_uses_absolute_configuration_path_during_initialization() -> Result<()> {
    first_document_uses_configuration_path_during_initialization(
        to_utf8_file_path_buf(uri!("biome.jsonc")).to_string(),
    )
    .await
}

#[tokio::test]
async fn first_document_uses_relative_configuration_path_during_initialization() -> Result<()> {
    first_document_uses_configuration_path_during_initialization("biome.jsonc".into()).await
}

async fn first_document_uses_configuration_path_during_initialization(
    configuration_path: String,
) -> Result<()> {
    let fs = MemoryFileSystem::default();
    fs.insert(
        to_utf8_file_path_buf(uri!("biome.json")),
        r#"{"javascript":{"formatter":{"quoteStyle":"double"}}}"#,
    );
    fs.insert(
        to_utf8_file_path_buf(uri!("biome.jsonc")),
        r#"{"javascript":{"formatter":{"quoteStyle":"single"}}}"#,
    );
    let factory = ServerFactory::new_with_fs(Arc::new(fs));
    let (mut service, client) = factory.create().into_inner();
    let (mut stream, mut sink) = client.split();
    let source = "const word = \"hello\";\n";

    service
        .call(
            Request::build("initialize")
                .id("initialize")
                .params(json!({
                    "capabilities": {},
                    "workspaceFolders": [{ "uri": uri!(""), "name": "workspace" }]
                }))
                .finish(),
        )
        .await?;

    let mut initialized =
        tokio::spawn(service.call(Request::build("initialized").params(json!({})).finish()));
    let initialization_request = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .context("initial configuration request timed out")?
        .context("missing initialization configuration request")?;
    assert_eq!(initialization_request.method(), "workspace/configuration");

    let opened = tokio::spawn(
        service.call(
            Request::build("textDocument/didOpen")
                .params(json!({
                    "textDocument": {
                        "uri": uri!("document.js"),
                        "languageId": "javascript",
                        "version": 1,
                        "text": source
                    }
                }))
                .finish(),
        ),
    );
    let open_request = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .context("document configuration request timed out")?
        .context("missing document configuration request")?;
    assert_eq!(open_request.method(), "workspace/configuration");

    let settings = json!([{ "configurationPath": configuration_path }]);
    sink.send(Response::from_ok(
        initialization_request.id().unwrap().clone(),
        settings.clone(),
    ))
    .await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            tokio::select! {
                result = &mut initialized => {
                    result??;
                    return Ok::<_, anyhow::Error>(());
                }
                request = stream.next() => {
                    let request = request.context("client stream ended during initialization")?;
                    if let Some(id) = request.id() {
                        assert!(matches!(request.method(), "client/registerCapability" | "client/unregisterCapability"));
                        sink.send(Response::from_ok(id.clone(), json!(null))).await?;
                    }
                }
            }
        }
    })
    .await
    .context("initialization timed out")??;

    sink.send(Response::from_ok(
        open_request.id().unwrap().clone(),
        settings,
    ))
    .await?;

    let (sender, mut receiver) = channel(CHANNEL_BUFFER_SIZE);
    let reader = tokio::spawn(client_handler(stream, sink, sender));
    tokio::time::timeout(Duration::from_secs(3), opened)
        .await
        .context("opening document timed out")???;
    wait_for_notification(&mut receiver, |notification| {
        matches!(notification, ServerNotification::PublishDiagnostics(params)
            if params.uri == uri!("document.js") && params.version == Some(1))
    })
    .await
    .context("missing document diagnostics")?;

    let response = service
        .call(
            Request::build("textDocument/formatting")
                .id("formatting")
                .params(json!({
                    "textDocument": { "uri": uri!("document.js") },
                    "options": { "tabSize": 2, "insertSpaces": true }
                }))
                .finish(),
        )
        .await?
        .context("missing formatting response")?;
    let (_, result) = response.into_parts();
    let edits: Vec<TextEdit> = from_value(result?)?;
    let mut formatted = source.to_string();
    for edit in edits.into_iter().rev() {
        assert_eq!(edit.range.start.line, 0);
        assert_eq!(edit.range.end.line, 0);
        formatted.replace_range(
            edit.range.start.character as usize..edit.range.end.character as usize,
            &edit.new_text,
        );
    }
    reader.abort();
    assert_eq!(formatted, "const word = 'hello';\n");

    Ok(())
}
