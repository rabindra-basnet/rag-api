pub mod base;
pub mod character;
pub mod code;
pub mod html;
pub mod json;
pub mod markdown;

pub use base::{split_text_on_tokens, Language, TextSplitter, Tokenizer, TokenTextSplitter};
pub use character::{CharacterTextSplitter, RecursiveCharacterTextSplitter};
pub use code::{
    CppTextSplitter, GoTextSplitter, HtmlTextSplitter, JavaTextSplitter, JSTextSplitter,
    LatexTextSplitter, MarkdownTextSplitter as CodeMarkdownTextSplitter, PhpTextSplitter,
    ProtoTextSplitter, PythonTextSplitter, RubyTextSplitter, RustTextSplitter, ScalaTextSplitter,
    SolTextSplitter, SwiftTextSplitter, RstTextSplitter,
};
pub use html::{HTMLHeaderTextSplitter, HTMLSectionSplitter, HeaderType as HtmlHeaderType};
pub use json::RecursiveJsonSplitter;
pub use markdown::{HeaderType as MdHeaderType, MarkdownHeaderTextSplitter, MarkdownTextSplitter};

/// Convenience used by the document ingestion pipeline. Mirrors
/// `RecursiveCharacterTextSplitter(chunk_size=, chunk_overlap=)` but with the
/// chunk length measured in tokenizer tokens.
pub fn chunk_text(text: &str, chunk_size: usize, chunk_overlap: usize) -> Vec<String> {
    let splitter = RecursiveCharacterTextSplitter::new(chunk_size, chunk_overlap);
    splitter.split_text(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_single_chunk() {
        assert_eq!(
            chunk_text("hello world", 100, 20),
            vec!["hello world".to_string()]
        );
    }

    #[test]
    fn long_text_multiple_chunks() {
        let text = "para one. ".repeat(200);
        let chunks = chunk_text(&text, 10, 2);
        assert!(chunks.len() > 1, "expected multiple chunks");
        assert!(chunks.iter().all(|c| !c.trim().is_empty()));
    }

    #[test]
    fn empty_text() {
        assert!(chunk_text("   ", 100, 20).is_empty());
    }

    #[test]
    fn recursive_respects_paragraphs() {
        let text = format!("{}\n\n{}", "a".repeat(50), "b".repeat(50));
        let chunks = chunk_text(&text, 20, 0);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn token_text_splitter_windows() {
        let text = "word ".repeat(100);
        let chunks = TokenTextSplitter::new(10, 0).split_text(&text);
        assert!(chunks.len() > 1);
    }

    #[test]
    fn code_splitter_runs() {
        let code = "def foo():\n    return 1\n\ndef bar():\n    return 2\n";
        let chunks = PythonTextSplitter::new(20, 0).split_text(code);
        assert!(chunks.len() >= 1);
    }

    #[test]
    fn json_splitter_runs() {
        let json = r#"{"a": 1, "b": {"c": 2, "d": [3, 4]}}"#;
        let chunks = RecursiveJsonSplitter::new(200).split_text(json);
        assert!(!chunks.is_empty());
    }
}
