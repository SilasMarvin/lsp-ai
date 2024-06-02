import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind
} from 'vscode-languageclient/node';
// import { v4 as uuidv4 } from 'uuid';

let client: LanguageClient;

// We set the default configurations here becauase VS Code tries to combine keys from defaults and user provided values which is annoying to deal with
const defaultServerConfiguration =
{
  "memory": {
    "file_store": {}
  },
  "models": {
    "model1": {
      "type": "open_ai",
      "chat_endpoint": "https://api.openai.com/v1/chat/completions",
      "model": "gpt-4o",
      "auth_token_env_var_name": "OPENAI_API_KEY"
    }
  }
}


const defaultGenerationConfiguration = {
  "model": "model1",
  "parameters": {
    "max_tokens": 128,
    "max_context": 1024,
    "messages": [
      {
        "role": "system",
        "content": "Instructions:\n- You are an AI programming assistant.\n- Given a piece of code with the cursor location marked by \"<CURSOR>\", replace \"<CURSOR>\" with the correct code or comment.\n- First, think step-by-step.\n- Describe your plan for what to build in pseudocode, written out in great detail.\n- Then output the code replacing the \"<CURSOR>\".\n- Ensure that your completion fits within the language context of the provided code snippet.\n\nRules:\n- Only respond with code or comments.\n- Only replace \"<CURSOR>\"; do not include any previously written code.\n- Never include \"<CURSOR>\" in your response.\n- If the cursor is within a comment, complete the comment meaningfully.\n- Handle ambiguous cases by providing the most contextually appropriate completion.\n- Be consistent with your responses."
      },
      {
        "role": "user",
        "content": "def greet(name):\n    print(f\"Hello, {<CURSOR>}\")"
      },
      {
        "role": "assistant",
        "content": "name"
      },
      {
        "role": "user",
        "content": "function sum(a, b) {\n    return a + <CURSOR>;\n}"
      },
      {
        "role": "assistant",
        "content": "b"
      },
      {
        "role": "user",
        "content": "fn multiply(a: i32, b: i32) -> i32 {\n    a * <CURSOR>\n}"
      },
      {
        "role": "assistant",
        "content": "b"
      },
      {
        "role": "user",
        "content": "# <CURSOR>\ndef add(a, b):\n    return a + b"
      },
      {
        "role": "assistant",
        "content": "Adds two numbers"
      },
      {
        "role": "user",
        "content": "# This function checks if a number is even\n<CURSOR>"
      },
      {
        "role": "assistant",
        "content": "def is_even(n):\n    return n % 2 == 0"
      },
      {
        "role": "user",
        "content": "public class HelloWorld {\n    public static void main(String[] args) {\n        System.out.println(\"Hello, <CURSOR>\");\n    }\n}"
      },
      {
        "role": "assistant",
        "content": "world"
      },
      {
        "role": "user",
        "content": "try:\n    # Trying to open a file\n    file = open(\"example.txt\", \"r\")\n    # <CURSOR>\nfinally:\n    file.close()"
      },
      {
        "role": "assistant",
        "content": "content = file.read()"
      },
      {
        "role": "user",
        "content": "#include <iostream>\nusing namespace std;\n\nint main() {\n    int a = 5, b = 10;\n    cout << \"Sum: \" << (a + <CURSOR>) << endl;\n    return 0;\n}"
      },
      {
        "role": "assistant",
        "content": "b"
      },
      {
        "role": "user",
        "content": "<!DOCTYPE html>\n<html>\n<head>\n    <title>My Page</title>\n</head>\n<body>\n    <h1>Welcome to My Page</h1>\n    <p>This is a sample page with a list of items:</p>\n    <ul>\n        <li>Item 1</li>\n        <li>Item 2</li>\n        <li><CURSOR></li>\n    </ul>\n</body>\n</html>"
      },
      {
        "role": "assistant",
        "content": "Item 3"
      },
      {
        "role": "user",
        "content": "{CODE}"
      }
    ]
  }
}

export function activate(context: vscode.ExtensionContext) {
  // Configure the server options
  const serverOptions: ServerOptions = {
    command: "lsp-ai",
    transport: TransportKind.stdio,
  };

  // Set the serverConfiguration
  let serverConfiguration;
  if (Object.keys(vscode.workspace.getConfiguration("lsp-ai").serverConfiguration).length != 0) {
    serverConfiguration = vscode.workspace.getConfiguration("lsp-ai").serverConfiguration;
  } else {
    serverConfiguration = defaultServerConfiguration;
  }

  // Set the generationConfiguration
  let generationConfiguration;
  if (Object.keys(vscode.workspace.getConfiguration("lsp-ai").generationConfiguration).length != 0) {
    generationConfiguration = vscode.workspace.getConfiguration("lsp-ai").generationConfiguration;
  } else {
    generationConfiguration = defaultGenerationConfiguration;
  }

  // Set the inlineCompletionConfiguration
  const inlineCompletionConfiguration = vscode.workspace.getConfiguration("lsp-ai").inlineCompletionConfiguration;

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file" }],
    initializationOptions: serverConfiguration
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
      position: editor.selection.active,
      model: generationConfiguration.model,
      parameters: generationConfiguration.parameters
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

  // Register as an inline completion provider
  // We want to sort of debounce queries
  let lastInlineCompletion = Date.now();
  let inlineCompletionRequestCounter = 0;
  vscode.languages.registerInlineCompletionItemProvider({ pattern: '**' },
    {
      provideInlineCompletionItems: async (document: vscode.TextDocument, position: vscode.Position) => {
        let params = {
          textDocument: {
            uri: document.uri.toString(),
          },
          position: position,
          model: generationConfiguration.model,
          parameters: generationConfiguration.parameters
        };

        inlineCompletionRequestCounter += 1;
        let localInlineCompletionRequestCounter = inlineCompletionRequestCounter;

        if ((Date.now() - lastInlineCompletion) / 1000 < 1 / inlineCompletionConfiguration["maxCompletionsPerSecond"]) {
          await new Promise(r => setTimeout(r, ((1 / inlineCompletionConfiguration["maxCompletionsPerSecond"]) - ((Date.now() - lastInlineCompletion) / 1000 )) * 1000));
          if (inlineCompletionRequestCounter == localInlineCompletionRequestCounter) {
            lastInlineCompletion = Date.now();
            const result = await client.sendRequest("textDocument/generation", params);
            return [new vscode.InlineCompletionItem(result["generatedText"])];
          } else {
            return [];
          }
        } else {
          lastInlineCompletion = Date.now();
          const result = await client.sendRequest("textDocument/generation", params);
          return [new vscode.InlineCompletionItem(result["generatedText"])];
        }
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
