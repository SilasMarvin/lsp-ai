!C I currently have the following in my Cargo.toml

[features]
default = []
all = ["dep:tree-sitter-python", "dep:tree-sitter-bash", "dep:tree-sitter-c", "dep:tree-sitter-cpp", "dep:tree-sitter-c-sharp", "dep:tree-sitter-css", "dep:tree-sitter-elixir", "dep:tree-sitter-erlang", "dep:tree-sitter-go", "dep:tree-sitter-html", "dep:tree-sitter-java", "dep:tree-sitter-javascript", "dep:tree-sitter-json", "dep:tree-sitter-rust", "dep:tree-sitter-haskell", "dep:tree-sitter-lua", "dep:tree-sitter-ocaml"]

Update the features to include a feature for each language that activates its relevant dependency


<|assistant|>
Here's how you can update your `Cargo.toml` to include a feature for each language that activates its relevant dependency:

```toml
[features]
default = []
all = ["python", "bash", "c", "cpp", "csharp", "css", "elixir", "erlang", "go", "html", "java", "javascript", "json", "rust", "haskell", "lua", "ocaml"]

python = ["dep:tree-sitter-python"]
bash = ["dep:tree-sitter-bash"]
c = ["dep:tree-sitter-c"]
cpp = ["dep:tree-sitter-cpp"]
csharp = ["dep:tree-sitter-c-sharp"]
css = ["dep:tree-sitter-css"]
elixir = ["dep:tree-sitter-elixir"]
erlang = ["dep:tree-sitter-erlang"]
go = ["dep:tree-sitter-go"]
html = ["dep:tree-sitter-html"]
java = ["dep:tree-sitter-java"]
javascript = ["dep:tree-sitter-javascript"]
json = ["dep:tree-sitter-json"]
rust = ["dep:tree-sitter-rust"]
haskell = ["dep:tree-sitter-haskell"]
lua = ["dep:tree-sitter-lua"]
ocaml = ["dep:tree-sitter-ocaml"]
```

This setup allows you to enable individual language support or use the `all` feature to enable all languages at once.

<|user|>
