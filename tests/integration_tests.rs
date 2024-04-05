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

// This completion sequence was created using helix with the lsp-ai analyzer and reading the logs
// It starts with a Python file:
// ```
// # Multiplies two numbers
// def multiply_two_numbers(x, y):
//
// # A singular test
// assert multiply_two_numbers(2, 3) == 6
// ```
// And has the following sequence of key strokes:
// o on line 2 (this creates an indented new line and enters insert mode)
// r
// e
// The sequence has:
// - 1 textDocument/DidOpen notification
// - 3 textDocument/didChange notifications
// - 1 textDocument/completion requests
// This test can fail if the model gives a different response than normal, but that seems reasonably unlikely
// I guess we should hardcode the seed or something if we want to do more of these
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

    let initialization_message = r##"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{"general":{"positionEncodings":["utf-8","utf-32","utf-16"]},"textDocument":{"codeAction":{"codeActionLiteralSupport":{"codeActionKind":{"valueSet":["","quickfix","refactor","refactor.extract","refactor.inline","refactor.rewrite","source","source.organizeImports"]}},"dataSupport":true,"disabledSupport":true,"isPreferredSupport":true,"resolveSupport":{"properties":["edit","command"]}},"completion":{"completionItem":{"deprecatedSupport":true,"insertReplaceSupport":true,"resolveSupport":{"properties":["documentation","detail","additionalTextEdits"]},"snippetSupport":true,"tagSupport":{"valueSet":[1]}},"completionItemKind":{}},"hover":{"contentFormat":["markdown"]},"inlayHint":{"dynamicRegistration":false},"publishDiagnostics":{"versionSupport":true},"rename":{"dynamicRegistration":false,"honorsChangeAnnotations":false,"prepareSupport":true},"signatureHelp":{"signatureInformation":{"activeParameterSupport":true,"documentationFormat":["markdown"],"parameterInformation":{"labelOffsetSupport":true}}}},"window":{"workDoneProgress":true},"workspace":{"applyEdit":true,"configuration":true,"didChangeConfiguration":{"dynamicRegistration":false},"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":false},"executeCommand":{"dynamicRegistration":false},"inlayHint":{"refreshSupport":false},"symbol":{"dynamicRegistration":false},"workspaceEdit":{"documentChanges":true,"failureHandling":"abort","normalizesLineEndings":false,"resourceOperations":["create","rename","delete"]},"workspaceFolders":true}},"clientInfo":{"name":"helix","version":"23.10 (f6021dd0)"},"processId":70007,"rootPath":"/Users/silas/Projects/Tests/lsp-ai-tests","rootUri":null,"workspaceFolders":[]},"id":0}"##;
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
        r##"{"jsonrpc":"2.0","id":1,"result":{"isIncomplete":false,"items":[{"filterText":"    re\n","kind":1,"label":"ai - turn x * y","textEdit":{"newText":"turn x * y","range":{"end":{"character":6,"line":2},"start":{"character":6,"line":2}}}}]}}"##
    );

    child.kill()?;
    Ok(())
}
