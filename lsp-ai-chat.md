!C How to get the line and character number of a substring in a Rust String

<|assistant|>
To get the line and character number of a substring in a Rust String, you can follow these steps:

1. Split the string into lines
2. Iterate through the lines
3. Find the substring in each line
4. Calculate the line and character number

Here's a function that does this:

```rust
fn find_substring_position(text: &str, substring: &str) -> Option<(usize, usize)> {
    for (line_number, line) in text.lines().enumerate() {
        if let Some(char_number) = line.find(substring) {
            return Some((line_number + 1, char_number + 1));
        }
    }
    None
}
```

Usage example:

```rust
fn main() {
    let text = "Hello, world!\nThis is a test.\nFind me here.";
    let substring = "test";
    
    match find_substring_position(text, substring) {
        Some((line, char)) => println!("Found at line {}, character {}", line, char),
        None => println!("Substring not found"),
    }
}
```

This function returns `Option<(usize, usize)>`, where the first `usize` is the line number and the second is the character number. Both are 1-indexed for readability. If the substring isn't found, it returns `None`.

<|user|>
