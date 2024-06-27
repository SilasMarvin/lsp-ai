use thiserror::Error;
use tree_sitter::{Tree, TreeCursor};

#[derive(Error, Debug)]
pub enum NewError {
    #[error("chunk_size must be greater than chunk_overlap")]
    SizeOverlapError,
}

#[derive(Error, Debug)]
pub enum SplitError {
    #[error("converting utf8 to str")]
    Utf8Error(#[from] core::str::Utf8Error),
}

pub struct TreeSitterCodeSplitter {
    chunk_size: usize,
    chunk_overlap: usize,
}

pub struct ByteRange {
    pub start_byte: usize,
    pub end_byte: usize,
}

impl ByteRange {
    fn new(start_byte: usize, end_byte: usize) -> Self {
        Self {
            start_byte,
            end_byte,
        }
    }
}

pub struct Chunk<'a> {
    pub text: &'a str,
    pub range: ByteRange,
}

impl<'a> Chunk<'a> {
    fn new(text: &'a str, range: ByteRange) -> Self {
        Self { text, range }
    }
}

impl TreeSitterCodeSplitter {
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Result<Self, NewError> {
        if chunk_overlap > chunk_size {
            Err(NewError::SizeOverlapError)
        } else {
            Ok(Self {
                chunk_size,
                chunk_overlap,
            })
        }
    }

    pub fn split<'c>(&self, tree: &Tree, utf8: &'c [u8]) -> Result<Vec<Chunk<'c>>, SplitError> {
        let cursor = tree.walk();
        Ok(self
            .split_recursive(cursor, utf8)?
            .into_iter()
            .rev()
            // Let's combine some of our smaller chunks together
            // We also want to do this in reverse as it (seems) to make more sense to combine code slices from bottom to top
            .try_fold(vec![], |mut acc, current| {
                if acc.is_empty() {
                    acc.push(current);
                    Ok::<_, SplitError>(acc)
                } else {
                    if acc.last().as_ref().unwrap().text.len() + current.text.len()
                        < self.chunk_size
                    {
                        let last = acc.pop().unwrap();
                        let text = std::str::from_utf8(
                            &utf8[current.range.start_byte..last.range.end_byte],
                        )?;
                        acc.push(Chunk::new(
                            text,
                            ByteRange::new(current.range.start_byte, last.range.end_byte),
                        ));
                    } else {
                        acc.push(current);
                    }
                    Ok(acc)
                }
            })?
            .into_iter()
            .rev()
            .collect())
    }

    fn split_recursive<'c>(
        &self,
        mut cursor: TreeCursor<'_>,
        utf8: &'c [u8],
    ) -> Result<Vec<Chunk<'c>>, SplitError> {
        let node = cursor.node();
        let text = node.utf8_text(utf8)?;

        // There are three cases:
        // 1. Is the current range of code smaller than the chunk_size? If so, return it
        // 2. If not, does the current node have children? If so, recursively walk down
        // 3. If not, we must split our current node
        let mut out = if text.chars().count() <= self.chunk_size {
            vec![Chunk::new(
                text,
                ByteRange::new(node.range().start_byte, node.range().end_byte),
            )]
        } else {
            let mut cursor_copy = cursor.clone();
            if cursor_copy.goto_first_child() {
                self.split_recursive(cursor_copy, utf8)?
            } else {
                let mut current_range =
                    ByteRange::new(node.range().start_byte, node.range().end_byte);
                let mut chunks = vec![];
                let mut current_chunk = text;
                loop {
                    if current_chunk.len() < self.chunk_size {
                        chunks.push(Chunk::new(current_chunk, current_range));
                        break;
                    } else {
                        let new_chunk = &current_chunk[0..self.chunk_size.min(current_chunk.len())];
                        let new_range = ByteRange::new(
                            current_range.start_byte,
                            current_range.start_byte + new_chunk.as_bytes().len(),
                        );
                        chunks.push(Chunk::new(new_chunk, new_range));
                        let new_current_chunk =
                            &current_chunk[self.chunk_size - self.chunk_overlap..];
                        let byte_diff =
                            current_chunk.as_bytes().len() - new_current_chunk.as_bytes().len();
                        current_range = ByteRange::new(
                            current_range.start_byte + byte_diff,
                            current_range.end_byte,
                        );
                        current_chunk = new_current_chunk
                    }
                }
                chunks
            }
        };
        if cursor.goto_next_sibling() {
            out.append(&mut self.split_recursive(cursor, utf8)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    #[test]
    fn test_split_rust() {
        let splitter = TreeSitterCodeSplitter::new(128, 0).unwrap();

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::language())
            .expect("Error loading Rust grammar");

        let source_code = r#"
#[derive(Debug)]
struct Rectangle {
    width: u32,
    height: u32,
}

impl Rectangle {
    fn area(&self) -> u32 {
        self.width * self.height
    }
}

fn main() {
    let rect1 = Rectangle {
        width: 30,
        height: 50,
    };

    println!(
        "The area of the rectangle is {} square pixels.",
        rect1.area()
    );
}
"#;
        let tree = parser.parse(source_code, None).unwrap();
        let chunks = splitter.split(&tree, source_code.as_bytes()).unwrap();
        assert_eq!(
            chunks[0].text,
            r#"#[derive(Debug)]
struct Rectangle {
    width: u32,
    height: u32,
}"#
        );
        assert_eq!(
            chunks[1].text,
            r#"impl Rectangle {
    fn area(&self) -> u32 {
        self.width * self.height
    }
}"#
        );
        assert_eq!(
            chunks[2].text,
            r#"fn main() {
    let rect1 = Rectangle {
        width: 30,
        height: 50,
    };"#
        );
        assert_eq!(
            chunks[3].text,
            r#"println!(
        "The area of the rectangle is {} square pixels.",
        rect1.area()
    );
}"#
        );
    }

    #[test]
    fn test_split_zig() {
        let splitter = TreeSitterCodeSplitter::new(128, 10).unwrap();

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::language())
            .expect("Error loading Rust grammar");

        let source_code = r#"
const std = @import("std");
const parseInt = std.fmt.parseInt;

std.debug.print("Here is a long string 1 ... Here is a long string 2 ... Here is a long string 3 ... Here is a long string 4 ... Here is a long string 5 ... Here is a long string 6 ... Here is a long string 7 ... Here is a long string 8 ... Here is a long string 9 ...", .{});

test "parse integers" {
    const input = "123 67 89,99";
    const ally = std.testing.allocator;

    var list = std.ArrayList(u32).init(ally);
    // Ensure the list is freed at scope exit.
    // Try commenting out this line!
    defer list.deinit();

    var it = std.mem.tokenizeAny(u8, input, " ,");
    while (it.next()) |num| {
        const n = try parseInt(u32, num, 10);
        try list.append(n);
    }

    const expected = [_]u32{ 123, 67, 89, 99 };

    for (expected, list.items) |exp, actual| {
        try std.testing.expectEqual(exp, actual);
    }
}
"#;
        let tree = parser.parse(source_code, None).unwrap();
        let chunks = splitter.split(&tree, source_code.as_bytes()).unwrap();

        assert_eq!(
            chunks[0].text,
            r#"const std = @import("std");
const parseInt = std.fmt.parseInt;

std.debug.print(""#
        );
        assert_eq!(
            chunks[1].text,
            r#"Here is a long string 1 ... Here is a long string 2 ... Here is a long string 3 ... Here is a long string 4 ... Here is a long s"#
        );
        assert_eq!(
            chunks[2].text,
            r#"s a long string 5 ... Here is a long string 6 ... Here is a long string 7 ... Here is a long string 8 ... Here is a long string "#
        );
        assert_eq!(chunks[3].text, r#"ng string 9 ...", .{});"#);
        assert_eq!(
            chunks[4].text,
            r#"test "parse integers" {
    const input = "123 67 89,99";
    const ally = std.testing.allocator;

    var list = std.ArrayList"#
        );
        assert_eq!(
            chunks[5].text,
            r#"(u32).init(ally);
    // Ensure the list is freed at scope exit.
    // Try commenting out this line!"#
        );
        assert_eq!(
            chunks[6].text,
            r#"defer list.deinit();

    var it = std.mem.tokenizeAny(u8, input, " ,");
    while (it.next()) |num"#
        );
        assert_eq!(
            chunks[7].text,
            r#"| {
        const n = try parseInt(u32, num, 10);
        try list.append(n);
    }

    const expected = [_]u32{ 123, 67, 89,"#
        );
        assert_eq!(
            chunks[8].text,
            r#"99 };

    for (expected, list.items) |exp, actual| {
        try std.testing.expectEqual(exp, actual);
    }
}"#
        );
    }
}
