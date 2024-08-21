use anyhow::Result;
use std::{
    io::{Read, Write},
    process::{ChildStdin, ChildStdout, Command, Stdio},
};

// Note if you get an empty response with no error, that typically means
// the language server died
fn read_response(stdout: &mut ChildStdout) -> Result<String> {
    let mut content_length = None;
    let mut buf = vec![];
    loop {
        let mut buf2 = vec![0];
        stdout.read_exact(&mut buf2)?;
        buf.push(buf2[0]);
        if let Some(content_length) = content_length {
            if buf.len() == content_length {
                break;
            }
        } else {
            let len = buf.len();
            if len > 4
                && buf[len - 4] == 13
                && buf[len - 3] == 10
                && buf[len - 2] == 13
                && buf[len - 1] == 10
            {
                content_length =
                    Some(String::from_utf8(buf[16..len - 4].to_vec())?.parse::<usize>()?);
                buf = vec![];
            }
        }
    }
    Ok(String::from_utf8(buf)?)
}

fn send_message(stdin: &mut ChildStdin, message: &str) -> Result<()> {
    stdin.write_all(format!("Content-Length: {}\r\n", message.as_bytes().len(),).as_bytes())?;
    stdin.write_all("\r\n".as_bytes())?;
    stdin.write_all(message.as_bytes())?;
    Ok(())
}

// This chat completion sequence was created using helix with lsp-ai and reading the logs
// It utilizes Ollama with llama3.1:8b and a temperature of 0
// It starts with a Python file:
// ```
// # Multiplies two numbers
// def multiply_two_numbers(x, y):
//
// # A singular test
// assert multiply_two_numbers(2, 3) == 6
//
// ```
// And has the following sequence of key strokes:
// o on line 2 (this creates an indented new line and enters insert mode)
// r
// e
// t
// u
// r
// n
// The sequence has:
// - 1 textDocument/DidOpen notification
// - 7 textDocument/didChange notifications
// - 1 textDocument/completion requests
#[test]
fn test_chat_completion_sequence() -> Result<()> {
    let mut child = Command::new("cargo")
        .arg("run")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    let initialization_message = r##"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{"general":{"positionEncodings":["utf-8","utf-32","utf-16"]},"textDocument":{"codeAction":{"codeActionLiteralSupport":{"codeActionKind":{"valueSet":["","quickfix","refactor","refactor.extract","refactor.inline","refactor.rewrite","source","source.organizeImports"]}},"dataSupport":true,"disabledSupport":true,"isPreferredSupport":true,"resolveSupport":{"properties":["edit","command"]}},"completion":{"completionItem":{"deprecatedSupport":true,"insertReplaceSupport":true,"resolveSupport":{"properties":["documentation","detail","additionalTextEdits"]},"snippetSupport":true,"tagSupport":{"valueSet":[1]}},"completionItemKind":{}},"hover":{"contentFormat":["markdown"]},"inlayHint":{"dynamicRegistration":false},"publishDiagnostics":{"tagSupport":{"valueSet":[1,2]},"versionSupport":true},"rename":{"dynamicRegistration":false,"honorsChangeAnnotations":false,"prepareSupport":true},"signatureHelp":{"signatureInformation":{"activeParameterSupport":true,"documentationFormat":["markdown"],"parameterInformation":{"labelOffsetSupport":true}}}},"window":{"workDoneProgress":true},"workspace":{"applyEdit":true,"configuration":true,"didChangeConfiguration":{"dynamicRegistration":false},"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":false},"executeCommand":{"dynamicRegistration":false},"fileOperations":{"didRename":true,"willRename":true},"inlayHint":{"refreshSupport":false},"symbol":{"dynamicRegistration":false},"workspaceEdit":{"documentChanges":true,"failureHandling":"abort","normalizesLineEndings":false,"resourceOperations":["create","rename","delete"]},"workspaceFolders":true}},"clientInfo":{"name":"helix","version":"24.3 (beb5afcb)"},"initializationOptions":{"completion":{"model":"model1","parameters":{"max_context":1024,"messages":[{"content":"Instructions:\n- You are an AI programming assistant.\n- Given a piece of code with the cursor location marked by \"<CURSOR>\", replace \"<CURSOR>\" with the correct code or comment.\n- First, think step-by-step.\n- Describe your plan for what to build in pseudocode, written out in great detail.\n- Then output the code replacing the \"<CURSOR>\"\n- Ensure that your completion fits within the language context of the provided code snippet (e.g., Python, JavaScript, Rust).\n\nRules:\n- Only respond with code or comments.\n- Only replace \"<CURSOR>\"; do not include any previously written code.\n- Never include \"<CURSOR>\" in your response\n- If the cursor is within a comment, complete the comment meaningfully.\n- Handle ambiguous cases by providing the most contextually appropriate completion.\n- Be consistent with your responses.","role":"system"},{"content":"def greet(name):\n    print(f\"Hello, {<CURSOR>}\")","role":"user"},{"content":"name","role":"assistant"},{"content":"function sum(a, b) {\n    return a + <CURSOR>;\n}","role":"user"},{"content":"b","role":"assistant"},{"content":"fn multiply(a: i32, b: i32) -> i32 {\n    a * <CURSOR>\n}","role":"user"},{"content":"b","role":"assistant"},{"content":"# <CURSOR>\ndef add(a, b):\n    return a + b","role":"user"},{"content":"Adds two numbers","role":"assistant"},{"content":"# This function checks if a number is even\n<CURSOR>","role":"user"},{"content":"def is_even(n):\n    return n % 2 == 0","role":"assistant"},{"content":"{CODE}","role":"user"}],"options":{"num_predict":32,"temperature":0}}},"memory":{"file_store":{}},"models":{"model1":{"model":"llama3:8b-instruct-q4_0","type":"ollama"}}},"processId":66009,"rootPath":"/home/silas/Projects/test","rootUri":null,"workspaceFolders":[]},"id":0}"##;
    send_message(&mut stdin, initialization_message)?;
    let _ = read_response(&mut stdout)?;

    send_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"languageId":"python","text":"# Multiplies two numbers\ndef multiply_two_numbers(x, y):\n\n# A singular test\nassert multiply_two_numbers(2, 3) == 6\n","uri":"file:///fake.py","version":0}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":31,"line":1},"start":{"character":31,"line":1}},"text":"\n    "}],"textDocument":{"uri":"file:///fake.py","version":1}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":4,"line":2},"start":{"character":4,"line":2}},"text":"r"}],"textDocument":{"uri":"file:///fake.py","version":2}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":5,"line":2},"start":{"character":5,"line":2}},"text":"e"}],"textDocument":{"uri":"file:///fake.py","version":3}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":6,"line":2},"start":{"character":6,"line":2}},"text":"t"}],"textDocument":{"uri":"file:///fake.py","version":4}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":7,"line":2},"start":{"character":7,"line":2}},"text":"u"}],"textDocument":{"uri":"file:///fake.py","version":5}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":8,"line":2},"start":{"character":8,"line":2}},"text":"r"}],"textDocument":{"uri":"file:///fake.py","version":6}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":9,"line":2},"start":{"character":9,"line":2}},"text":"n"}],"textDocument":{"uri":"file:///fake.py","version":7}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/completion","params":{"position":{"character":10,"line":2},"textDocument":{"uri":"file:///fake.py"}},"id":1}"##,
    )?;

    let output = read_response(&mut stdout)?;
    assert_eq!(
        output,
        r##"{"jsonrpc":"2.0","id":1,"result":{"isIncomplete":false,"items":[{"filterText":"    return","kind":1,"label":"ai - x * y","textEdit":{"newText":"x * y","range":{"end":{"character":10,"line":2},"start":{"character":10,"line":2}}}}]}}"##
    );

    child.kill()?;
    Ok(())
}

// This FIM completion sequence was created using helix with lsp-ai and reading the logs
// It utilizes Ollama with deepseek-coder:1.3b-base and a temperature of 0
// It starts with a Python file:
// ```
// # Multiplies two numbers
// def multiply_two_numbers(x, y):
//
// # A singular test
// assert multiply_two_numbers(2, 3) == 6
//
// ```
// And has the following sequence of key strokes:
// o on line 2 (this creates an indented new line and enters insert mode)
// r
// e
// The sequence has:
// - 1 textDocument/DidOpen notification
// - 3 textDocument/didChange notifications
// - 1 textDocument/completion requests
#[test]
fn test_fim_completion_sequence() -> Result<()> {
    let mut child = Command::new("cargo")
        .arg("run")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    let initialization_message = r##"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{"general":{"positionEncodings":["utf-8","utf-32","utf-16"]},"textDocument":{"codeAction":{"codeActionLiteralSupport":{"codeActionKind":{"valueSet":["","quickfix","refactor","refactor.extract","refactor.inline","refactor.rewrite","source","source.organizeImports"]}},"dataSupport":true,"disabledSupport":true,"isPreferredSupport":true,"resolveSupport":{"properties":["edit","command"]}},"completion":{"completionItem":{"deprecatedSupport":true,"insertReplaceSupport":true,"resolveSupport":{"properties":["documentation","detail","additionalTextEdits"]},"snippetSupport":true,"tagSupport":{"valueSet":[1]}},"completionItemKind":{}},"hover":{"contentFormat":["markdown"]},"inlayHint":{"dynamicRegistration":false},"publishDiagnostics":{"tagSupport":{"valueSet":[1,2]},"versionSupport":true},"rename":{"dynamicRegistration":false,"honorsChangeAnnotations":false,"prepareSupport":true},"signatureHelp":{"signatureInformation":{"activeParameterSupport":true,"documentationFormat":["markdown"],"parameterInformation":{"labelOffsetSupport":true}}}},"window":{"workDoneProgress":true},"workspace":{"applyEdit":true,"configuration":true,"didChangeConfiguration":{"dynamicRegistration":false},"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":false},"executeCommand":{"dynamicRegistration":false},"fileOperations":{"didRename":true,"willRename":true},"inlayHint":{"refreshSupport":false},"symbol":{"dynamicRegistration":false},"workspaceEdit":{"documentChanges":true,"failureHandling":"abort","normalizesLineEndings":false,"resourceOperations":["create","rename","delete"]},"workspaceFolders":true}},"clientInfo":{"name":"helix","version":"24.3 (beb5afcb)"},"initializationOptions":{"completion":{"model":"model1","parameters":{"fim":{"end":"<｜fim▁end｜>","middle":"<｜fim▁hole｜>","start":"<｜fim▁begin｜>"},"max_context":1024,"options":{"num_predict":32,"temperature":0}}},"memory":{"file_store":{}},"models":{"model1":{"model":"deepseek-coder:1.3b-base","type":"ollama"}}},"processId":50347,"rootPath":"/home/silas/Projects/test","rootUri":null,"workspaceFolders":[]},"id":0}"##;
    send_message(&mut stdin, initialization_message)?;
    let _ = read_response(&mut stdout)?;

    send_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"languageId":"python","text":"# Multiplies two numbers\ndef multiply_two_numbers(x, y):\n\n# A singular test\nassert multiply_two_numbers(2, 3) == 6\n","uri":"file:///fake.py","version":0}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":31,"line":1},"start":{"character":31,"line":1}},"text":"\n    "}],"textDocument":{"uri":"file:///fake.py","version":1}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":4,"line":2},"start":{"character":4,"line":2}},"text":"r"}],"textDocument":{"uri":"file:///fake.py","version":2}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":5,"line":2},"start":{"character":5,"line":2}},"text":"e"}],"textDocument":{"uri":"file:///fake.py","version":3}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/completion","params":{"position":{"character":6,"line":2},"textDocument":{"uri":"file:///fake.py"}},"id":1}"##,
    )?;

    let output = read_response(&mut stdout)?;
    assert_eq!(
        output,
        r##"{"jsonrpc":"2.0","id":1,"result":{"isIncomplete":false,"items":[{"filterText":"    re","kind":1,"label":"ai - turn x * y","textEdit":{"newText":"turn x * y","range":{"end":{"character":6,"line":2},"start":{"character":6,"line":2}}}}]}}"##
    );

    child.kill()?;
    Ok(())
}

// This completion sequence was created using helix with lsp-ai and reading the logs
// It utilizes Ollama with deepseek-coder:1.3b-base and a temperature of 0
// It starts with a Python file:
// ```
// # Multiplies two numbers
// def multiply_two_numbers(x, y):
//
// ```
// And has the following sequence of key strokes:
// o on line 2 (this creates an indented new line and enters insert mode)
// r
// e
// t
// u
// r
// n
// The sequence has:
// - 1 textDocument/DidOpen notification
// - 7 textDocument/didChange notifications
// - 1 textDocument/completion requests
#[test]
fn test_completion_sequence() -> Result<()> {
    let mut child = Command::new("cargo")
        .arg("run")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    let initialization_message = r##"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{"general":{"positionEncodings":["utf-8","utf-32","utf-16"]},"textDocument":{"codeAction":{"codeActionLiteralSupport":{"codeActionKind":{"valueSet":["","quickfix","refactor","refactor.extract","refactor.inline","refactor.rewrite","source","source.organizeImports"]}},"dataSupport":true,"disabledSupport":true,"isPreferredSupport":true,"resolveSupport":{"properties":["edit","command"]}},"completion":{"completionItem":{"deprecatedSupport":true,"insertReplaceSupport":true,"resolveSupport":{"properties":["documentation","detail","additionalTextEdits"]},"snippetSupport":true,"tagSupport":{"valueSet":[1]}},"completionItemKind":{}},"hover":{"contentFormat":["markdown"]},"inlayHint":{"dynamicRegistration":false},"publishDiagnostics":{"tagSupport":{"valueSet":[1,2]},"versionSupport":true},"rename":{"dynamicRegistration":false,"honorsChangeAnnotations":false,"prepareSupport":true},"signatureHelp":{"signatureInformation":{"activeParameterSupport":true,"documentationFormat":["markdown"],"parameterInformation":{"labelOffsetSupport":true}}}},"window":{"workDoneProgress":true},"workspace":{"applyEdit":true,"configuration":true,"didChangeConfiguration":{"dynamicRegistration":false},"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":false},"executeCommand":{"dynamicRegistration":false},"fileOperations":{"didRename":true,"willRename":true},"inlayHint":{"refreshSupport":false},"symbol":{"dynamicRegistration":false},"workspaceEdit":{"documentChanges":true,"failureHandling":"abort","normalizesLineEndings":false,"resourceOperations":["create","rename","delete"]},"workspaceFolders":true}},"clientInfo":{"name":"helix","version":"24.3 (beb5afcb)"},"initializationOptions":{"completion":{"model":"model1","parameters":{"max_context":1024,"options":{"num_predict":32,"temperature":0}}},"memory":{"file_store":{}},"models":{"model1":{"model":"deepseek-coder:1.3b-base","type":"ollama"}}},"processId":62322,"rootPath":"/home/silas/Projects/test","rootUri":null,"workspaceFolders":[]},"id":0}"##;
    send_message(&mut stdin, initialization_message)?;
    let _ = read_response(&mut stdout)?;

    send_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"languageId":"python","text":"# Multiplies two numbers\ndef multiply_two_numbers(x, y):\n","uri":"file:///fake.py","version":0}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":31,"line":1},"start":{"character":31,"line":1}},"text":"\n    "}],"textDocument":{"uri":"file:///fake.py","version":1}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":4,"line":2},"start":{"character":4,"line":2}},"text":"r"}],"textDocument":{"uri":"file:///fake.py","version":2}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":5,"line":2},"start":{"character":5,"line":2}},"text":"e"}],"textDocument":{"uri":"file:///fake.py","version":3}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":6,"line":2},"start":{"character":6,"line":2}},"text":"t"}],"textDocument":{"uri":"file:///fake.py","version":4}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":7,"line":2},"start":{"character":7,"line":2}},"text":"u"}],"textDocument":{"uri":"file:///fake.py","version":5}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":8,"line":2},"start":{"character":8,"line":2}},"text":"r"}],"textDocument":{"uri":"file:///fake.py","version":6}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":9,"line":2},"start":{"character":9,"line":2}},"text":"n"}],"textDocument":{"uri":"file:///fake.py","version":7}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/completion","params":{"position":{"character":10,"line":2},"textDocument":{"uri":"file:///fake.py"}},"id":1}"##,
    )?;

    let output = read_response(&mut stdout)?;
    assert_eq!(
        output,
        r##"{"jsonrpc":"2.0","id":1,"result":{"isIncomplete":false,"items":[{"filterText":"    return","kind":1,"label":"ai -  x * y","textEdit":{"newText":" x * y","range":{"end":{"character":10,"line":2},"start":{"character":10,"line":2}}}}]}}"##
    );

    child.kill()?;
    Ok(())
}

// This chat sequence was created using helix with lsp-ai and reading the logs
// It utilizes Ollama with llama3:8b-instruct-q4_0 and a temperature of 0
// It starts with a Markdown file:
// ```
// !C Who are
// ```
// And has the following sequence of key strokes:
// A on line 1 (this enters insert mode at the end of line 1)
// (space)
// y
// o
// u
// (esc)
// (run code action for chatting)
// ge (goes to end of file)
// o (inserts a new line below cursor)
// ?
// (esc)
// (run code action for chatting)
#[test]
fn test_chat_sequence() -> Result<()> {
    let mut child = Command::new("cargo")
        .arg("run")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    let initialization_message = r##"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{"general":{"positionEncodings":["utf-8","utf-32","utf-16"]},"textDocument":{"codeAction":{"codeActionLiteralSupport":{"codeActionKind":{"valueSet":["","quickfix","refactor","refactor.extract","refactor.inline","refactor.rewrite","source","source.organizeImports"]}},"dataSupport":true,"disabledSupport":true,"isPreferredSupport":true,"resolveSupport":{"properties":["edit","command"]}},"completion":{"completionItem":{"deprecatedSupport":true,"insertReplaceSupport":true,"resolveSupport":{"properties":["documentation","detail","additionalTextEdits"]},"snippetSupport":true,"tagSupport":{"valueSet":[1]}},"completionItemKind":{}},"formatting":{"dynamicRegistration":false},"hover":{"contentFormat":["markdown"]},"inlayHint":{"dynamicRegistration":false},"publishDiagnostics":{"tagSupport":{"valueSet":[1,2]},"versionSupport":true},"rename":{"dynamicRegistration":false,"honorsChangeAnnotations":false,"prepareSupport":true},"signatureHelp":{"signatureInformation":{"activeParameterSupport":true,"documentationFormat":["markdown"],"parameterInformation":{"labelOffsetSupport":true}}}},"window":{"workDoneProgress":true},"workspace":{"applyEdit":true,"configuration":true,"didChangeConfiguration":{"dynamicRegistration":false},"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":false},"executeCommand":{"dynamicRegistration":false},"fileOperations":{"didRename":true,"willRename":true},"inlayHint":{"refreshSupport":false},"symbol":{"dynamicRegistration":false},"workspaceEdit":{"documentChanges":true,"failureHandling":"abort","normalizesLineEndings":false,"resourceOperations":["create","rename","delete"]},"workspaceFolders":true}},"clientInfo":{"name":"helix","version":"24.7 (0d62656c)"},"initializationOptions":{"chat":[{"action_display_name":"Chat","model":"model1","parameters":{"max_context":4096,"max_tokens":1024,"messages":[{"content":"You are a code assistant chatbot. The user will ask you for assistance coding and you will do you best to answer succinctly and accurately","role":"system"}],"options":{"temperature":0}},"trigger":"!C"}],"memory":{"file_store":{}},"models":{"model1":{"model":"llama3:8b-instruct-q4_0","type":"ollama"}}},"processId":50522,"rootPath":"/Users/silas/Projects/Tests/lsp-ai-tests","rootUri":null,"workspaceFolders":[]},"id":0}"##;
    send_message(&mut stdin, initialization_message)?;
    let _ = read_response(&mut stdout)?;

    send_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"languageId":"markdown","text":"!C Who are\n","uri":"file:///fake.md","version":0}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":10,"line":0},"start":{"character":10,"line":0}},"text":" "}],"textDocument":{"uri":"file:///fake.md","version":1}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":11,"line":0},"start":{"character":11,"line":0}},"text":"y"}],"textDocument":{"uri":"file:///fake.md","version":2}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":12,"line":0},"start":{"character":12,"line":0}},"text":"o"}],"textDocument":{"uri":"file:///fake.md","version":3}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":13,"line":0},"start":{"character":13,"line":0}},"text":"u"}],"textDocument":{"uri":"file:///fake.md","version":4}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/codeAction","params":{"context":{"diagnostics":[],"triggerKind":1},"range":{"end":{"character":0,"line":1},"start":{"character":14,"line":0}},"textDocument":{"uri":"file:///fake.md"}},"id":3}"##,
    )?;

    // Test that our action is present
    let output = read_response(&mut stdout)?;
    assert_eq!(
        output,
        r##"{"jsonrpc":"2.0","id":3,"result":[{"data":{"range":{"end":{"character":0,"line":1},"start":{"character":14,"line":0}},"text_document":{"uri":"file:///fake.md"}},"title":"Chat"}]}"##
    );

    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"codeAction/resolve","params":{"data":{"range":{"end":{"character":0,"line":1},"start":{"character":14,"line":0}},"text_document":{"uri":"file:///fake.md"}},"title":"Chat"},"id":4}"##,
    )?;

    // Test that we get the corret model output
    let output = read_response(&mut stdout)?;
    assert_eq!(
        output,
        r##"{"jsonrpc":"2.0","id":4,"result":{"edit":{"changes":{"file:///fake.md":[{"newText":"\n\n<|assistant|>\nI'm CodePal, your friendly AI code assistant! I'm here to help you with any programming-related questions or problems you might have. Whether you're a beginner or an experienced developer, I'll do my best to provide clear and concise answers to get you back on track. What can I help you with today?\n\n<|user|>\n","range":{"end":{"character":12,"line":1},"start":{"character":12,"line":1}}}]}},"title":"Chat"}}"##
    );

    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":0,"line":1},"start":{"character":0,"line":1}},"text":"\n\n<|assistant|>\nI'm CodePal, your friendly AI code assistant! I'm here to help you with any programming-related questions or problems you might have. Whether you're a beginner or an experienced developer, I'll do my best to provide clear and concise answers to get you back on track. What can I help you with today?\n\n<|user|>\n"}],"textDocument":{"uri":"file:///fake.md","version":5}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":8,"line":6},"start":{"character":8,"line":6}},"text":"\n"}],"textDocument":{"uri":"file:///fake.md","version":6}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"contentChanges":[{"range":{"end":{"character":0,"line":7},"start":{"character":0,"line":7}},"text":"?"}],"textDocument":{"uri":"file:///fake.md","version":7}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/codeAction","params":{"context":{"diagnostics":[],"triggerKind":1},"range":{"end":{"character":0,"line":8},"start":{"character":1,"line":7}},"textDocument":{"uri":"file:///fake.md"}},"id":5}"##,
    )?;

    // Test that our action is present
    let output = read_response(&mut stdout)?;
    assert_eq!(
        output,
        r##"{"jsonrpc":"2.0","id":5,"result":[{"data":{"range":{"end":{"character":0,"line":8},"start":{"character":1,"line":7}},"text_document":{"uri":"file:///fake.md"}},"title":"Chat"}]}"##
    );

    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"codeAction/resolve","params":{"data":{"range":{"end":{"character":0,"line":8},"start":{"character":1,"line":7}},"text_document":{"uri":"file:///fake.md"}},"title":"Chat"},"id":6}"##,
    )?;

    // Test that we get the correct model output
    let output = read_response(&mut stdout)?;
    assert_eq!(
        output,
        r##"{"jsonrpc":"2.0","id":6,"result":{"edit":{"changes":{"file:///fake.md":[{"newText":"\n\n<|assistant|>\nIt seems like you didn't ask a question! That's okay, I'm here to help whenever you're ready. If you have any programming-related questions or need assistance with a specific coding problem, feel free to ask me anything!\n\n<|user|>\n","range":{"end":{"character":1,"line":8},"start":{"character":1,"line":8}}}]}},"title":"Chat"}}"##
    );

    child.kill()?;
    Ok(())
}

// This custom action completion sequence was created using helix with lsp-ai and reading the logs
// It utilizes Ollama with llama3.1:8b and a temperature of 0
// It starts with a Python file:
// ```
// def fib(n):
// ```
// And has the following sequence of key strokes:
// gl (goes to end of line with cursor on ":")
// l (moves cursor to position after ":")
// (run code action for Completion)
#[test]
fn test_completion_action_sequence() -> Result<()> {
    let mut child = Command::new("cargo")
        .arg("run")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    let initialization_message = r##"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{"general":{"positionEncodings":["utf-8","utf-32","utf-16"]},"textDocument":{"codeAction":{"codeActionLiteralSupport":{"codeActionKind":{"valueSet":["","quickfix","refactor","refactor.extract","refactor.inline","refactor.rewrite","source","source.organizeImports"]}},"dataSupport":true,"disabledSupport":true,"isPreferredSupport":true,"resolveSupport":{"properties":["edit","command"]}},"completion":{"completionItem":{"deprecatedSupport":true,"insertReplaceSupport":true,"resolveSupport":{"properties":["documentation","detail","additionalTextEdits"]},"snippetSupport":true,"tagSupport":{"valueSet":[1]}},"completionItemKind":{}},"formatting":{"dynamicRegistration":false},"hover":{"contentFormat":["markdown"]},"inlayHint":{"dynamicRegistration":false},"publishDiagnostics":{"tagSupport":{"valueSet":[1,2]},"versionSupport":true},"rename":{"dynamicRegistration":false,"honorsChangeAnnotations":false,"prepareSupport":true},"signatureHelp":{"signatureInformation":{"activeParameterSupport":true,"documentationFormat":["markdown"],"parameterInformation":{"labelOffsetSupport":true}}}},"window":{"workDoneProgress":true},"workspace":{"applyEdit":true,"configuration":true,"didChangeConfiguration":{"dynamicRegistration":false},"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":false},"executeCommand":{"dynamicRegistration":false},"fileOperations":{"didRename":true,"willRename":true},"inlayHint":{"refreshSupport":false},"symbol":{"dynamicRegistration":false},"workspaceEdit":{"documentChanges":true,"failureHandling":"abort","normalizesLineEndings":false,"resourceOperations":["create","rename","delete"]},"workspaceFolders":true}},"clientInfo":{"name":"helix","version":"24.7 (0d62656c)"},"initializationOptions":{"actions":[{"action_display_name":"Complete","model":"model1","parameters":{"max_context":4096,"max_tokens":4096,"messages":[{"content":"You are an AI coding assistant. Your task is to complete code snippets. The user's cursor position is marked by \"<CURSOR>\". Follow these steps:\n\n1. Analyze the code context and the cursor position.\n2. Provide your chain of thought reasoning, wrapped in <reasoning> tags. Include thoughts about the cursor position, what needs to be completed, and any necessary formatting.\n3. Determine the appropriate code to complete the current thought, including finishing partial words or lines.\n4. Replace \"<CURSOR>\" with the necessary code, ensuring proper formatting and line breaks.\n5. Wrap your code solution in <answer> tags.\n\nYour response should always include both the reasoning and the answer. Pay special attention to completing partial words or lines before adding new lines of code.\n\n<examples>\n<example>\nUser input:\n--main.py--\n# A function that reads in user inpu<CURSOR>\n\nResponse:\n<reasoning>\n1. The cursor is positioned after \"inpu\" in a comment describing a function that reads user input.\n2. We need to complete the word \"input\" in the comment first.\n3. After completing the comment, we should add a new line before defining the function.\n4. The function should use Python's built-in `input()` function to read user input.\n5. We'll name the function descriptively and include a return statement.\n</reasoning>\n\n<answer>t\ndef read_user_input():\n    user_input = input(\"Enter your input: \")\n    return user_input\n</answer>\n</example>\n\n<example>\nUser input:\n--main.py--\ndef fibonacci(n):\n    if n <= 1:\n        return n\n    else:\n        re<CURSOR>\n\n\nResponse:\n<reasoning>\n1. The cursor is positioned after \"re\" in the 'else' clause of a recursive Fibonacci function.\n2. We need to complete the return statement for the recursive case.\n3. The \"re\" already present likely stands for \"return\", so we'll continue from there.\n4. The Fibonacci sequence is the sum of the two preceding numbers.\n5. We should return the sum of fibonacci(n-1) and fibonacci(n-2).\n</reasoning>\n\n<answer>turn fibonacci(n-1) + fibonacci(n-2)</answer>\n</example>\n</examples>\n","role":"system"},{"content":"{CODE}","role":"user"}],"options":{"temperature":0}},"post_process":{"extractor":"(?s)<answer>(.*?)</answer>"}}],"memory":{"file_store":{}},"models":{"model1":{"model":"llama3.1:8b","type":"ollama"}}},"processId":55832,"rootPath":"/Users/silas/Projects/Tests/lsp-ai-tests","rootUri":null,"workspaceFolders":[]},"id":0}"##;
    send_message(&mut stdin, initialization_message)?;
    let _ = read_response(&mut stdout)?;

    send_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"languageId":"python","text":"def fib(n):\n","uri":"file:///fake.py","version":0}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/codeAction","params":{"context":{"diagnostics":[],"triggerKind":1},"range":{"end":{"character":0,"line":1},"start":{"character":11,"line":0}},"textDocument":{"uri":"file:///fake.py"}},"id":1}"##,
    )?;

    // Test that our action is present
    let output = read_response(&mut stdout)?;
    assert_eq!(
        output,
        r##"{"jsonrpc":"2.0","id":1,"result":[{"data":{"range":{"end":{"character":0,"line":1},"start":{"character":11,"line":0}},"text_document":{"uri":"file:///fake.py"}},"title":"Complete"}]}"##
    );

    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"codeAction/resolve","params":{"data":{"range":{"end":{"character":0,"line":1},"start":{"character":11,"line":0}},"text_document":{"uri":"file:///fake.py"}},"title":"Complete"},"id":2}"##,
    )?;

    // Test that we get the corret model output
    let output = read_response(&mut stdout)?;
    assert_eq!(
        output,
        r##"{"jsonrpc":"2.0","id":2,"result":{"edit":{"changes":{"file:///fake.py":[{"newText":"\n    if n <= 1:\n        return n\n    else:\n        return fib(n-1) + fib(n-2)","range":{"end":{"character":0,"line":1},"start":{"character":11,"line":0}}}]}},"title":"Complete"}}"##
    );

    child.kill()?;
    Ok(())
}

// This custom action refactor sequence was created using helix with lsp-ai and reading the logs
// It utilizes Ollama with llama3.1:8b and a temperature of 0
// It starts with a Python file:
// ```
// # The fibonacci recursively
// def fib(n):
//     if n <= 1:
//         return n
//     a, b = 0, 1
//     for _ in range(2, n + 1):
//         a, b = b, a + b
//     return b
// ```
// And has the following sequence of key strokes:
// % (selects all text)
// (run code action for Refactoring)
#[test]
fn test_refactor_action_sequence() -> Result<()> {
    let mut child = Command::new("cargo")
        .arg("run")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    let initialization_message = r##"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{"general":{"positionEncodings":["utf-8","utf-32","utf-16"]},"textDocument":{"codeAction":{"codeActionLiteralSupport":{"codeActionKind":{"valueSet":["","quickfix","refactor","refactor.extract","refactor.inline","refactor.rewrite","source","source.organizeImports"]}},"dataSupport":true,"disabledSupport":true,"isPreferredSupport":true,"resolveSupport":{"properties":["edit","command"]}},"completion":{"completionItem":{"deprecatedSupport":true,"insertReplaceSupport":true,"resolveSupport":{"properties":["documentation","detail","additionalTextEdits"]},"snippetSupport":true,"tagSupport":{"valueSet":[1]}},"completionItemKind":{}},"formatting":{"dynamicRegistration":false},"hover":{"contentFormat":["markdown"]},"inlayHint":{"dynamicRegistration":false},"publishDiagnostics":{"tagSupport":{"valueSet":[1,2]},"versionSupport":true},"rename":{"dynamicRegistration":false,"honorsChangeAnnotations":false,"prepareSupport":true},"signatureHelp":{"signatureInformation":{"activeParameterSupport":true,"documentationFormat":["markdown"],"parameterInformation":{"labelOffsetSupport":true}}}},"window":{"workDoneProgress":true},"workspace":{"applyEdit":true,"configuration":true,"didChangeConfiguration":{"dynamicRegistration":false},"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":false},"executeCommand":{"dynamicRegistration":false},"fileOperations":{"didRename":true,"willRename":true},"inlayHint":{"refreshSupport":false},"symbol":{"dynamicRegistration":false},"workspaceEdit":{"documentChanges":true,"failureHandling":"abort","normalizesLineEndings":false,"resourceOperations":["create","rename","delete"]},"workspaceFolders":true}},"clientInfo":{"name":"helix","version":"24.7 (0d62656c)"},"initializationOptions":{"actions":[{"action_display_name":"Refactor","model":"model1","parameters":{"max_context":4096,"max_tokens":4096,"messages":[{"content":"You are an AI coding assistant specializing in code refactoring. Your task is to analyze the given code snippet and provide a refactored version. Follow these steps:\n\n1. Analyze the code context and structure.\n2. Identify areas for improvement, such as code efficiency, readability, or adherence to best practices.\n3. Provide your chain of thought reasoning, wrapped in <reasoning> tags. Include your analysis of the current code and explain your refactoring decisions.\n4. Rewrite the entire code snippet with your refactoring applied.\n5. Wrap your refactored code solution in <answer> tags.\n\nYour response should always include both the reasoning and the refactored code.\n\n<examples>\n<example>\nUser input:\ndef calculate_total(items):\n    total = 0\n    for item in items:\n        total = total + item['price'] * item['quantity']\n    return total\n\n\nResponse:\n<reasoning>\n1. The function calculates the total cost of items based on price and quantity.\n2. We can improve readability and efficiency by:\n   a. Using a more descriptive variable name for the total.\n   b. Utilizing the sum() function with a generator expression.\n   c. Using augmented assignment (+=) if we keep the for loop.\n3. We'll implement the sum() function approach for conciseness.\n4. We'll add a type hint for better code documentation.\n</reasoning>\n<answer>\nfrom typing import List, Dict\n\ndef calculate_total(items: List[Dict[str, float]]) -> float:\n    return sum(item['price'] * item['quantity'] for item in items)\n</answer>\n</example>\n\n<example>\nUser input:\ndef is_prime(n):\n    if n < 2:\n        return False\n    for i in range(2, n):\n        if n % i == 0:\n            return False\n    return True\n\n\nResponse:\n<reasoning>\n1. This function checks if a number is prime, but it's not efficient for large numbers.\n2. We can improve it by:\n   a. Adding an early return for 2, the only even prime number.\n   b. Checking only odd numbers up to the square root of n.\n   c. Using a more efficient range (start at 3, step by 2).\n3. We'll also add a type hint for better documentation.\n4. The refactored version will be more efficient for larger numbers.\n</reasoning>\n<answer>\nimport math\n\ndef is_prime(n: int) -> bool:\n    if n < 2:\n        return False\n    if n == 2:\n        return True\n    if n % 2 == 0:\n        return False\n    \n    for i in range(3, int(math.sqrt(n)) + 1, 2):\n        if n % i == 0:\n            return False\n    return True\n</answer>\n</example>\n</examples>","role":"system"},{"content":"{SELECTED_TEXT}","role":"user"}],"options":{"temperature":0}},"post_process":{"extractor":"(?s)<answer>(.*?)</answer>"}}],"memory":{"file_store":{}},"models":{"model1":{"model":"llama3.1:8b","type":"ollama"}}},"processId":56258,"rootPath":"/Users/silas/Projects/Tests/lsp-ai-tests","rootUri":null,"workspaceFolders":[]},"id":0}"##;
    send_message(&mut stdin, initialization_message)?;
    let _ = read_response(&mut stdout)?;

    send_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"languageId":"python","text":"# The fibonacci recursively\ndef fib(n):\n    if n <= 1:\n        return n\n    a, b = 0, 1\n    for _ in range(2, n + 1):\n        a, b = b, a + b\n    return b\n","uri":"file:///fake.py","version":0}}}"##,
    )?;
    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"textDocument/codeAction","params":{"context":{"diagnostics":[],"triggerKind":1},"range":{"end":{"character":0,"line":8},"start":{"character":0,"line":0}},"textDocument":{"uri":"file:///fake.py"}},"id":1}"##,
    )?;

    // Test that our action is present
    let output = read_response(&mut stdout)?;
    assert_eq!(
        output,
        r##"{"jsonrpc":"2.0","id":1,"result":[{"data":{"range":{"end":{"character":0,"line":8},"start":{"character":0,"line":0}},"text_document":{"uri":"file:///fake.py"}},"title":"Refactor"}]}"##
    );

    send_message(
        &mut stdin,
        r##"{"jsonrpc":"2.0","method":"codeAction/resolve","params":{"data":{"range":{"end":{"character":0,"line":8},"start":{"character":0,"line":0}},"text_document":{"uri":"file:///fake.py"}},"title":"Refactor"},"id":2}"##,
    )?;

    // Test that we get the corret model output
    let output = read_response(&mut stdout)?;
    assert_eq!(
        output,
        r##"{"jsonrpc":"2.0","id":2,"result":{"edit":{"changes":{"file:///fake.py":[{"newText":"\nfrom typing import Dict\n\ndef fib(n: int) -> int:\n    memo: Dict[int, int] = {0: 0, 1: 1}\n    \n    def calculate_fibonacci(k: int) -> int:\n        if k not in memo:\n            memo[k] = calculate_fibonacci(k - 1) + calculate_fibonacci(k - 2)\n        return memo[k]\n    \n    return calculate_fibonacci(n)\n","range":{"end":{"character":0,"line":8},"start":{"character":0,"line":0}}}]}},"title":"Refactor"}}"##
    );

    child.kill()?;
    Ok(())
}
