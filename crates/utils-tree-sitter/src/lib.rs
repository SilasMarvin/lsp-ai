use thiserror::Error;
use tree_sitter::{LanguageError, Parser, Tree};

#[derive(Error, Debug)]
pub enum TreeSitterUtilsError {
    #[error("loading grammer error: {0}")]
    LoadingGrammer(#[from] LanguageError),
    #[error("no parser found for extension: {0}")]
    NoParserFoundForExtension(String),
    #[error("no parser found for extension: {0}")]
    NoLanguageFoundForExtension(String),
    #[error("failed to parse tree for: {0}")]
    Parsing(String),
}

fn get_extension_for_language(extension: &str) -> Result<String, TreeSitterUtilsError> {
    Ok(match extension {
        "py" => "Python",
        "rs" => "Rust",
        // "zig" => "Zig",
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
            return Err(TreeSitterUtilsError::NoLanguageFoundForExtension(
                extension.to_string(),
            ))
        }
    }
    .to_string())
}

pub fn get_parser_for_extension(extension: &str) -> Result<Parser, TreeSitterUtilsError> {
    let language = get_extension_for_language(extension)?;
    let mut parser = Parser::new();
    match language.as_str() {
        #[cfg(any(feature = "all", feature = "python"))]
        "Python" => parser.set_language(&tree_sitter_python::language())?,
        #[cfg(any(feature = "all", feature = "rust"))]
        "Rust" => parser.set_language(&tree_sitter_rust::language())?,
        // #[cfg(any(feature = "all", feature = "zig"))]
        // "Zig" => parser.set_language(&tree_sitter_zig::language())?,
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
            return Err(TreeSitterUtilsError::NoParserFoundForExtension(
                language.to_string(),
            ))
        }
    }
    Ok(parser)
}

pub fn parse_tree(
    uri: &str,
    contents: &str,
    old_tree: Option<&Tree>,
) -> Result<Tree, TreeSitterUtilsError> {
    let path = std::path::Path::new(uri);
    let extension = path.extension().map(|x| x.to_string_lossy());
    let extension = extension.as_deref().unwrap_or("");
    let mut parser = get_parser_for_extension(extension)?;
    parser
        .parse(contents, old_tree)
        .ok_or_else(|| TreeSitterUtilsError::Parsing(uri.to_owned()))
}
