use crate::service::chunker_service::base::{
    find_separator, split_with_regex, TextSplitter,
};

const DEFAULT_SEPARATORS: &[&str] = &["\n\n", "\n", " ", ""];

/// Port of langchain `RecursiveCharacterTextSplitter`.
pub struct RecursiveCharacterTextSplitter {
    base: TextSplitter,
    separators: Vec<String>,
}

impl RecursiveCharacterTextSplitter {
    /// LangChain-style constructor: `RecursiveCharacterTextSplitter(chunk_size, chunk_overlap)`.
    /// Chunk length is measured in tokenizer tokens.
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        let separators = DEFAULT_SEPARATORS.iter().map(|s| s.to_string()).collect();
        Self {
            base: TextSplitter::with_token_length(chunk_size, chunk_overlap),
            separators,
        }
    }

    pub fn with_char_length(chunk_size: usize, chunk_overlap: usize) -> Self {
        let separators = DEFAULT_SEPARATORS.iter().map(|s| s.to_string()).collect();
        Self {
            base: TextSplitter::with_length(chunk_size, chunk_overlap, |s| s.chars().count()),
            separators,
        }
    }

    /// Build a splitter that uses language-specific separators (e.g. code).
    pub fn with_separators(
        chunk_size: usize,
        chunk_overlap: usize,
        separators: Vec<String>,
        token_length: bool,
    ) -> Self {
        let base = if token_length {
            TextSplitter::with_token_length(chunk_size, chunk_overlap)
        } else {
            TextSplitter::with_length(chunk_size, chunk_overlap, |s| s.chars().count())
        };
        Self { base, separators }
    }

    pub fn split_text(&self, text: &str) -> Vec<String> {
        self._split_text(text, &self.separators)
    }

    fn _split_text(&self, text: &str, separators: &[String]) -> Vec<String> {
        let mut final_chunks = Vec::new();

        let (separator, new_separators) = find_separator(text, separators);
        let sep = if separator.is_empty() {
            String::new()
        } else {
            separator.clone()
        };

        let splits: Vec<String> = if separator.is_empty() {
            text.chars().map(|c| c.to_string()).collect()
        } else {
            split_with_regex(text, &sep, self.base.keep_separator)
        };

        let mut good_splits: Vec<String> = Vec::new();
        for s in splits {
            if self.base.length(&s) < self.base.chunk_size {
                good_splits.push(s);
            } else {
                if !good_splits.is_empty() {
                    final_chunks.extend(self.base.merge(&good_splits, &sep));
                    good_splits.clear();
                }
                if new_separators.is_empty() {
                    final_chunks.push(s);
                } else {
                    final_chunks.extend(self._split_text(&s, new_separators));
                }
            }
        }
        if !good_splits.is_empty() {
            final_chunks.extend(self.base.merge(&good_splits, &sep));
        }

        final_chunks.retain(|c| !c.trim().is_empty());
        final_chunks
    }
}

/// Port of langchain `CharacterTextSplitter`.
pub struct CharacterTextSplitter {
    base: TextSplitter,
    separator: String,
}

impl CharacterTextSplitter {
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        Self::with_separator(chunk_size, chunk_overlap, "\n\n".to_string(), true)
    }

    pub fn with_separator(
        chunk_size: usize,
        chunk_overlap: usize,
        separator: String,
        token_length: bool,
    ) -> Self {
        let base = if token_length {
            TextSplitter::with_token_length(chunk_size, chunk_overlap)
        } else {
            TextSplitter::with_length(chunk_size, chunk_overlap, |s| s.chars().count())
        };
        Self { base, separator }
    }

    pub fn split_text(&self, text: &str) -> Vec<String> {
        let splits = split_with_regex(text, &self.separator, self.base.keep_separator);
        self.base.merge(&splits, &self.separator)
    }
}
