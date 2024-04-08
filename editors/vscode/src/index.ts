import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind
} from 'vscode-languageclient/node';
// import { v4 as uuidv4 } from 'uuid';

let client: LanguageClient;

export function activate(context: vscode.ExtensionContext) {
  // Configure the server options
  let serverOptions: ServerOptions = {
    command: "lsp-ai",
    transport: TransportKind.stdio,
  };

  // Options to control the language client
  let clientOptions: LanguageClientOptions = {
    documentSelector: [{ pattern: "**" }]
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

  // Register generate function
  const generateCommand = 'lsp-ai.generation';
  const generateCommandHandler = (editor: vscode.TextEditor) => {
    let params = {
      textDocument: {
        uri: editor.document.uri.toString(),
      },
      position: editor.selection.active
    };
    client.sendRequest("textDocument/generation", params).then(result => {
      editor.edit((edit) => {
        edit.insert(editor.selection.active, result["generatedText"]);
      });
    }).catch(error => {
      console.error("Error making generate request", error);
    });
  };
  context.subscriptions.push(vscode.commands.registerTextEditorCommand(generateCommand, generateCommandHandler));

  vscode.languages.registerInlineCompletionItemProvider({ pattern: '**' },
    {
      provideInlineCompletionItems: async (document: vscode.TextDocument, position: vscode.Position) => {
        let params = {
          textDocument: {
            uri: document.uri.toString(),
          },
          position: position
        };
        const result = await client.sendRequest("textDocument/generation", params);
        return [new vscode.InlineCompletionItem(result["generatedText"])];
      }
    }
  );
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
