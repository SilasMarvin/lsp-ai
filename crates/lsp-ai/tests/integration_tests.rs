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
// It utilizes Ollama with llama3:8b-instruct-q4_0 and a temperature of 0
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
