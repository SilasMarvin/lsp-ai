use anyhow::Result;

use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId};
use lsp_types::{
    request::Completion, CompletionOptions, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    RenameFilesParams, ServerCapabilities, TextDocumentSyncKind,
};
use parking_lot::Mutex;
use std::{
    sync::{mpsc, Arc},
    thread,
};
use tracing::error;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

mod configuration;
mod custom_requests;
mod memory_backends;
mod memory_worker;
mod template;
mod transformer_backends;
mod transformer_worker;
mod utils;

use configuration::Configuration;
use custom_requests::generate::Generate;
use memory_backends::MemoryBackend;
use transformer_backends::TransformerBackend;
use transformer_worker::{CompletionRequest, GenerateRequest, WorkerRequest};

use crate::{
    custom_requests::generate_stream::GenerateStream, transformer_worker::GenerateStreamRequest,
};

fn notification_is<N: lsp_types::notification::Notification>(notification: &Notification) -> bool {
    notification.method == N::METHOD
}

fn request_is<R: lsp_types::request::Request>(request: &Request) -> bool {
    request.method == R::METHOD
}

fn cast<R>(req: Request) -> Result<(RequestId, R::Params), ExtractError<Request>>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    req.extract(R::METHOD)
}

fn main() -> Result<()> {
    // Builds a tracing subscriber from the `LSP_AI_LOG` environment variable
    // If the variables value is malformed or missing, sets the default log level to ERROR
    FmtSubscriber::builder()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_env("LSP_AI_LOG"))
        .init();

    let (connection, io_threads) = Connection::stdio();
    let server_capabilities = serde_json::to_value(ServerCapabilities {
        completion_provider: Some(CompletionOptions::default()),
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        ..Default::default()
    })?;
    let initialization_args = connection.initialize(server_capabilities)?;

    main_loop(connection, initialization_args)?;
    io_threads.join()?;
    Ok(())
}

fn main_loop(connection: Connection, args: serde_json::Value) -> Result<()> {
    // Build our configuration
    let configuration = Configuration::new(args)?;

    // Wrap the connection for sharing between threads
    let connection = Arc::new(connection);

    // Our channel we use to communicate with our transformer_worker
    let last_worker_request = Arc::new(Mutex::new(None));

    // TODO:
    // Both of these workers should be resiliant to errors
    // If they have an error they should just try to restart. It should be logged as an error, but it shouldn't kill the process

    // Setup our memory_worker
    // TODO: Setup some kind of error handler
    // Set the memory_backend
    // The channel we use to communicate with our memory_worker
    let (memory_tx, memory_rx) = mpsc::channel();
    let memory_backend: Box<dyn MemoryBackend + Send + Sync> = configuration.clone().try_into()?;
    thread::spawn(move || memory_worker::run(memory_backend, memory_rx));

    // Setup our transformer_worker
    // Thread local variables
    // TODO: Setup some kind of handler for errors here
    // Set the transformer_backend
    let transformer_backend: Box<dyn TransformerBackend + Send + Sync> =
        configuration.clone().try_into()?;
    let thread_last_worker_request = last_worker_request.clone();
    let thread_connection = connection.clone();
    let thread_memory_tx = memory_tx.clone();
    thread::spawn(move || {
        transformer_worker::run(
            transformer_backend,
            thread_memory_tx,
            thread_last_worker_request,
            thread_connection,
        )
    });

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                // Right now each if / else basically does the same thing,
                // but this may change soon so it is worth making it a little
                // more verbose than it needs to be now
                if request_is::<Completion>(&req) {
                    match cast::<Completion>(req) {
                        Ok((id, params)) => {
                            let mut lcr = last_worker_request.lock();
                            let completion_request = CompletionRequest::new(id, params);
                            *lcr = Some(WorkerRequest::Completion(completion_request));
                        }
                        Err(err) => error!("{err:?}"),
                    }
                } else if request_is::<Generate>(&req) {
                    match cast::<Generate>(req) {
                        Ok((id, params)) => {
                            let mut lcr = last_worker_request.lock();
                            let completion_request = GenerateRequest::new(id, params);
                            *lcr = Some(WorkerRequest::Generate(completion_request));
                        }
                        Err(err) => error!("{err:?}"),
                    }
                } else if request_is::<GenerateStream>(&req) {
                    match cast::<GenerateStream>(req) {
                        Ok((id, params)) => {
                            let mut lcr = last_worker_request.lock();
                            let completion_request = GenerateStreamRequest::new(id, params);
                            *lcr = Some(WorkerRequest::GenerateStream(completion_request));
                        }
                        Err(err) => error!("{err:?}"),
                    }
                } else {
                    error!("lsp-ai currently only supports textDocument/completion, textDocument/generate and textDocument/generateStream")
                }
            }
            Message::Notification(not) => {
                if notification_is::<lsp_types::notification::DidOpenTextDocument>(&not) {
                    let params: DidOpenTextDocumentParams = serde_json::from_value(not.params)?;
                    memory_tx.send(memory_worker::WorkerRequest::DidOpenTextDocument(params))?;
                } else if notification_is::<lsp_types::notification::DidChangeTextDocument>(&not) {
                    let params: DidChangeTextDocumentParams = serde_json::from_value(not.params)?;
                    memory_tx.send(memory_worker::WorkerRequest::DidChangeTextDocument(params))?;
                } else if notification_is::<lsp_types::notification::DidRenameFiles>(&not) {
                    let params: RenameFilesParams = serde_json::from_value(not.params)?;
                    memory_tx.send(memory_worker::WorkerRequest::DidRenameFiles(params))?;
                }
            }
            _ => (),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_backends::Prompt;
    use serde_json::json;

    //////////////////////////////////////
    //////////////////////////////////////
    /// Some basic gguf model tests //////
    //////////////////////////////////////
    //////////////////////////////////////

    #[tokio::test]
    async fn completion_with_default_arguments() {
        let args = json!({});
        let configuration = Configuration::new(args).unwrap();
        let backend: Box<dyn TransformerBackend + Send + Sync> =
            configuration.clone().try_into().unwrap();
        let prompt = Prompt::new("".to_string(), "def fibn".to_string());
        let response = backend.do_completion(&prompt).await.unwrap();
        assert!(!response.insert_text.is_empty())
    }

    #[tokio::test]
    async fn completion_with_custom_gguf_model() {
        let args = json!({
            "initializationOptions": {
                "memory": {
                    "file_store": {}
                },
                "macos": {
                    "model_gguf": {
                        "repository": "TheBloke/deepseek-coder-6.7B-instruct-GGUF",
                        "name": "deepseek-coder-6.7b-instruct.Q5_K_S.gguf",
                        "max_new_tokens": {
                            "completion": 32,
                            "generation": 256,
                        },
                        // "fim": {
                        //     "start": "<fim_prefix>",
                        //     "middle": "<fim_suffix>",
                        //     "end": "<fim_middle>"
                        // },
                        // "chat": {
                        //     "completion": [
                        //         {
                        //             "role": "system",
                        //             "content": "You are a code completion chatbot. Use the following context to complete the next segement of code. Keep your response brief. Do not produce any text besides code. \n\n{context}",
                        //         },
                        //         {
                        //             "role": "user",
                        //             "content": "Complete the following code: \n\n{code}"
                        //         }
                        //     ],
                        //     "generation": [
                        //         {
                        //             "role": "system",
                        //             "content": "You are a code completion chatbot. Use the following context to complete the next segement of code. \n\n{context}",
                        //         },
                        //         {
                        //             "role": "user",
                        //             "content": "Complete the following code: \n\n{code}"
                        //         }
                        //     ],
                        //     // "chat_template": "{% if not add_generation_prompt is defined %}\n{% set add_generation_prompt = false %}\n{% endif %}\n{%- set ns = namespace(found=false) -%}\n{%- for message in messages -%}\n    {%- if message['role'] == 'system' -%}\n        {%- set ns.found = true -%}\n    {%- endif -%}\n{%- endfor -%}\n{{bos_token}}{%- if not ns.found -%}\n{{'You are an AI programming assistant, utilizing the Deepseek Coder model, developed by Deepseek Company, and you only answer questions related to computer science. For politically sensitive questions, security and privacy issues, and other non-computer science questions, you will refuse to answer\\n'}}\n{%- endif %}\n{%- for message in messages %}\n    {%- if message['role'] == 'system' %}\n{{ message['content'] }}\n    {%- else %}\n        {%- if message['role'] == 'user' %}\n{{'### Instruction:\\n' + message['content'] + '\\n'}}\n        {%- else %}\n{{'### Response:\\n' + message['content'] + '\\n<|EOT|>\\n'}}\n        {%- endif %}\n    {%- endif %}\n{%- endfor %}\n{% if add_generation_prompt %}\n{{'### Response:'}}\n{% endif %}"
                        // },
                        "n_ctx": 2048,
                        "n_gpu_layers": 35,
                    }
                },
            }
        });
        let configuration = Configuration::new(args).unwrap();
        let backend: Box<dyn TransformerBackend + Send + Sync> =
            configuration.clone().try_into().unwrap();
        let prompt = Prompt::new("".to_string(), "def fibn".to_string());
        let response = backend.do_completion(&prompt).await.unwrap();
        assert!(!response.insert_text.is_empty());
    }
}
