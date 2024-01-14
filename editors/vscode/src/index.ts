import { workspace, ExtensionContext } from 'vscode';

import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(_context: ExtensionContext) {
  console.log("\n\nIN THE ACTIVATE FUNCTION\n\n");

  // Configure the server options
  let serverOptions: ServerOptions = {
    command: "lsp-ai", 
    transport: TransportKind.stdio,
  };

  // Options to control the language client
  let clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'python' }],
    synchronize: {
      // Notify the server about file changes to '.clientrc files contained in the workspace
      fileEvents: workspace.createFileSystemWatcher('**/.clientrc')
    }
  };

  // Create the language client and start the client
  client = new LanguageClient(
    'lsp-ai',
    'lsp-ai',
    serverOptions,
    clientOptions
  );

  console.log("\n\nSTARTING THE CLIENT\n\n");

  // Start the client. This will also launch the server
  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
