import * as vscode  from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: vscode.ExtensionContext) {
  // Configure the server options
  let serverOptions: ServerOptions = {
    command: "lsp-ai", 
    transport: TransportKind.stdio,
  };

  // Options to control the language client
  let clientOptions: LanguageClientOptions = {
    documentSelector: [{ pattern: "**" }], 
  };

  // Create the language client and start the client
  client = new LanguageClient(
    'lsp-ai',
    'lsp-ai',
    serverOptions,
    clientOptions
  );

  // Start the client. This will also launch the server
  client.start();

  client.onRequest("textDocument/completion", (params) => {
    console.log("HERE WE GO");
    console.log(params);
  });

  // Register functions
  const command = 'lsp-ai.generate';
  const commandHandler = () => {
    const editor = vscode.window.activeTextEditor;
    console.log("SENDING REQUEST FOR GENERATE");
    console.log(editor);
    let params = {
      textDocument: {
        uri: editor.document.uri.toString(),
      },
      position: editor.selection.active
    };
    console.log(params);
    client.sendRequest("textDocument/generate", params).then(result => {
      console.log(result);
    }).catch(error => {
      console.error(error);
    });
  };
  context.subscriptions.push(vscode.commands.registerCommand(command, commandHandler));
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
