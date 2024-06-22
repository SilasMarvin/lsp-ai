use thiserror::Error;
use tree_sitter::{LanguageError, Parser};

#[derive(Error, Debug)]
pub enum GetParserError {
    #[error("no parser found for extension")]
    NoParserFoundForExtension(String),
    #[error("no parser found for extension")]
    NoLanguageFoundForExtension(String),
    #[error("loading grammer")]
    LoadingGrammer(#[from] LanguageError),
}

fn get_extension_for_language(extension: &str) -> Result<String, GetParserError> {
    Ok(match extension {
        "py" => "Python",
        "rs" => "Rust",
        "zig" => "Zig",
        "sh" => "Bash",
        "c" => "C",
        "cpp" => "C++",
        "cs" => "C#",
        "css" => "CSS",
        "ex" => "Elixir",
        "erl" => "Erlang",
        "go" => "Go",
        "html" => "HTML",
        "java" => "Java",
        "js" => "JavaScript",
        "json" => "JSON",
        "hs" => "Haskell",
        "lua" => "Lua",
        "ml" => "OCaml",
        _ => {
            return Err(GetParserError::NoLanguageFoundForExtension(
                extension.to_string(),
            ))
        }
    }
    .to_string())
}

pub fn get_parser_for_extension(extension: &str) -> Result<Parser, GetParserError> {
    let language = get_extension_for_language(extension)?;
    let mut parser = Parser::new();
    match language.as_str() {
        #[cfg(any(feature = "all", feature = "python"))]
        "Python" => parser.set_language(&tree_sitter_python::language())?,
        #[cfg(any(feature = "all", feature = "rust"))]
        "Rust" => parser.set_language(&tree_sitter_rust::language())?,
        #[cfg(any(feature = "all", feature = "zig"))]
        "Zig" => parser.set_language(&tree_sitter_zig::language())?,
        #[cfg(any(feature = "all", feature = "bash"))]
        "Bash" => parser.set_language(&tree_sitter_bash::language())?,
        #[cfg(any(feature = "all", feature = "c"))]
        "C" => parser.set_language(&tree_sitter_c::language())?,
        #[cfg(any(feature = "all", feature = "cpp"))]
        "C++" => parser.set_language(&tree_sitter_cpp::language())?,
        #[cfg(any(feature = "all", feature = "c-sharp"))]
        "C#" => parser.set_language(&tree_sitter_c_sharp::language())?,
        #[cfg(any(feature = "all", feature = "css"))]
        "CSS" => parser.set_language(&tree_sitter_css::language())?,
        #[cfg(any(feature = "all", feature = "elixir"))]
        "Elixir" => parser.set_language(&tree_sitter_elixir::language())?,
        #[cfg(any(feature = "all", feature = "erlang"))]
        "Erlang" => parser.set_language(&tree_sitter_erlang::language())?,
        #[cfg(any(feature = "all", feature = "go"))]
        "Go" => parser.set_language(&tree_sitter_go::language())?,
        #[cfg(any(feature = "all", feature = "html"))]
        "HTML" => parser.set_language(&tree_sitter_html::language())?,
        #[cfg(any(feature = "all", feature = "java"))]
        "Java" => parser.set_language(&tree_sitter_java::language())?,
        #[cfg(any(feature = "all", feature = "javascript"))]
        "JavaScript" => parser.set_language(&tree_sitter_javascript::language())?,
        #[cfg(any(feature = "all", feature = "json"))]
        "JSON" => parser.set_language(&tree_sitter_json::language())?,
        #[cfg(any(feature = "all", feature = "haskell"))]
        "Haskell" => parser.set_language(&tree_sitter_haskell::language())?,
        #[cfg(any(feature = "all", feature = "lua"))]
        "Lua" => parser.set_language(&tree_sitter_lua::language())?,
        #[cfg(any(feature = "all", feature = "ocaml"))]
        "OCaml" => parser.set_language(&tree_sitter_ocaml::language_ocaml())?,
        _ => {
            return Err(GetParserError::NoParserFoundForExtension(
                language.to_string(),
            ))
        }
    }
    Ok(parser)
}
