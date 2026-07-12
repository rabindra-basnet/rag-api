use std::sync::OnceLock;

use tokenizers::Tokenizer as HfTokenizer;

const DEFAULT_TOKENIZER_MODEL: &str = "gpt2";

// ---------------------------------------------------------------------------
// Lazy HF tokenizer (mirrors langchain's `from_huggingface_tokenizer`).
// ---------------------------------------------------------------------------

static TOKENIZER: OnceLock<Option<HfTokenizer>> = OnceLock::new();

fn tokenizer_model() -> &'static str {
    crate::config::parameter::try_get()
        .map(|c| c.tokenizer_model.as_str())
        .unwrap_or(DEFAULT_TOKENIZER_MODEL)
}

fn load_tokenizer() -> Option<HfTokenizer> {
    match HfTokenizer::from_pretrained(tokenizer_model(), None) {
        Ok(t) => Some(t),
        Err(e) => {
            tracing::warn!(
                error = %e,
                model = tokenizer_model(),
                "failed to load HF tokenizer; falling back to char-based length estimate"
            );
            None
        }
    }
}

fn tokenizer() -> Option<&'static HfTokenizer> {
    TOKENIZER.get_or_init(load_tokenizer).as_ref()
}

/// Token count of `text`, or a chars/4 estimate when no tokenizer is available.
pub fn token_count(text: &str) -> usize {
    match tokenizer() {
        Some(t) => t
            .encode(text, false)
            .map(|e| e.get_ids().len())
            .unwrap_or_else(|_| fallback_len(text)),
        None => fallback_len(text),
    }
}

fn fallback_len(text: &str) -> usize {
    (text.chars().count() + 3) / 4
}

// ---------------------------------------------------------------------------
// Tokenizer (protocol equivalent).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub enum Tokenizer {
    HuggingFace,
}

impl Tokenizer {
    /// Split `text` into token-bounded chunks (port of `split_text_on_tokens`).
    pub fn split_text_on_tokens(&self, text: &str, chunk_size: usize, chunk_overlap: usize) -> Vec<String> {
        split_text_on_tokens(text, chunk_size, chunk_overlap)
    }
}

// ---------------------------------------------------------------------------
// Language enum (port of langchain `base.Language`).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Cpp,
    Go,
    Java,
    Js,
    Php,
    Proto,
    Python,
    Rst,
    Ruby,
    Rust,
    Scala,
    Swift,
    Markdown,
    Latex,
    Html,
    Sol,
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Cpp => "cpp",
            Language::Go => "go",
            Language::Java => "java",
            Language::Js => "js",
            Language::Php => "php",
            Language::Proto => "proto",
            Language::Python => "python",
            Language::Rst => "rst",
            Language::Ruby => "ruby",
            Language::Rust => "rust",
            Language::Scala => "scala",
            Language::Swift => "swift",
            Language::Markdown => "markdown",
            Language::Latex => "latex",
            Language::Html => "html",
            Language::Sol => "sol",
        }
    }

    /// LangChain `LANGUAGE_SEPARATORS` mapping.
    pub fn separators(&self) -> Vec<String> {
        match self {
            Language::Cpp => vec![
                "\nclass ".into(),
                "\nstruct ".into(),
                "\nenum ".into(),
                "\nnamespace ".into(),
                "\ninterface ".into(),
                "\npublic ".into(),
                "\nprivate ".into(),
                "\nprotected ".into(),
                "\ntemplate ".into(),
                "\ntypedef ".into(),
                "\nusing ".into(),
                "\n#include".into(),
                "\n#define".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Go => vec![
                "\nfunc ".into(),
                "\ntype ".into(),
                "\nvar ".into(),
                "\nconst ".into(),
                "\nimport ".into(),
                "\npackage ".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Java => vec![
                "\nclass ".into(),
                "\ninterface ".into(),
                "\nenum ".into(),
                "\npublic ".into(),
                "\nprivate ".into(),
                "\nprotected ".into(),
                "\nstatic ".into(),
                "\nfinal ".into(),
                "\nvoid ".into(),
                "\nint ".into(),
                "\nfloat ".into(),
                "\ndouble ".into(),
                "\nboolean ".into(),
                "\nString ".into(),
                "\npackage ".into(),
                "\nimport ".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Js => vec![
                "\nfunction ".into(),
                "\nconst ".into(),
                "\nlet ".into(),
                "\nvar ".into(),
                "\nclass ".into(),
                "\ninterface ".into(),
                "\nexport ".into(),
                "\nimport ".into(),
                "\n/*".into(),
                "\n//".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Php => vec![
                "\nfunction ".into(),
                "\nclass ".into(),
                "\ninterface ".into(),
                "\nabstract ".into(),
                "\nfinal ".into(),
                "\npublic ".into(),
                "\nprivate ".into(),
                "\nprotected ".into(),
                "\nnamespace ".into(),
                "\nuse ".into(),
                "\n<?php".into(),
                "\n?>".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Proto => vec![
                "\nmessage ".into(),
                "\nservice ".into(),
                "\nenum ".into(),
                "\npackage ".into(),
                "\nimport ".into(),
                "\nsyntax ".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Python => vec![
                "\nclass ".into(),
                "\ndef ".into(),
                "\n\tdef ".into(),
                "\nasync def ".into(),
                "\n@".into(),
                "\nwith ".into(),
                "\nfor ".into(),
                "\nif ".into(),
                "\nelif ".into(),
                "\nelse ".into(),
                "\ntry ".into(),
                "\nexcept ".into(),
                "\nfinally ".into(),
                "\nwhile ".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Rst => vec![
                "\n===".into(),
                "\n---".into(),
                "\n***".into(),
                "\n+++".into(),
                "\n```".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Ruby => vec![
                "\nclass ".into(),
                "\nmodule ".into(),
                "\ndef ".into(),
                "\nattr_".into(),
                "\npublic ".into(),
                "\nprivate ".into(),
                "\nprotected ".into(),
                "\nrequire ".into(),
                "\ninclude ".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Rust => vec![
                "\nfn ".into(),
                "\nstruct ".into(),
                "\nenum ".into(),
                "\ntrait ".into(),
                "\nimpl ".into(),
                "\nmod ".into(),
                "\npub ".into(),
                "\nuse ".into(),
                "\nconst ".into(),
                "\nstatic ".into(),
                "\nmacro_rules! ".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Scala => vec![
                "\nclass ".into(),
                "\nobject ".into(),
                "\ntrait ".into(),
                "\ndef ".into(),
                "\nval ".into(),
                "\nvar ".into(),
                "\npackage ".into(),
                "\nimport ".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Swift => vec![
                "\nclass ".into(),
                "\nstruct ".into(),
                "\nenum ".into(),
                "\nprotocol ".into(),
                "\nextension ".into(),
                "\nfunc ".into(),
                "\nlet ".into(),
                "\nvar ".into(),
                "\nimport ".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Markdown => vec![
                "\n#{1,6} ".into(),
                "\n```".into(),
                "\n\\*\\*\\*+".into(),
                "\n---+".into(),
                "\n___+".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Latex => vec![
                "\n\\chapter".into(),
                "\n\\section".into(),
                "\n\\subsection".into(),
                "\n\\subsubsection".into(),
                "\n\\paragraph".into(),
                "\n\\subparagraph".into(),
                "\n\\begin{".into(),
                "\n\\end{".into(),
                "\n\\item".into(),
                "\n\\usepackage".into(),
                "\n\\documentclass".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Html => vec![
                "\n<div".into(),
                "\n</div>".into(),
                "\n<span".into(),
                "\n</span>".into(),
                "\n<p".into(),
                "\n</p>".into(),
                "\n<h1".into(),
                "\n<h2".into(),
                "\n<h3".into(),
                "\n<h4".into(),
                "\n<h5".into(),
                "\n<h6".into(),
                "\n<li".into(),
                "\n<tr".into(),
                "\n<td".into(),
                "\n<th".into(),
                "\n<body".into(),
                "\n<head".into(),
                "\n<html".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
            Language::Sol => vec![
                "\ncontract ".into(),
                "\nlibrary ".into(),
                "\ninterface ".into(),
                "\nfunction ".into(),
                "\nevent ".into(),
                "\nmodifier ".into(),
                "\npragma ".into(),
                "\nimport ".into(),
                "\n\n".into(),
                "\n".into(),
                " ".into(),
                "".into(),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Base TextSplitter (port of langchain `base.TextSplitter`).
// ---------------------------------------------------------------------------

pub struct TextSplitter {
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub length_function: fn(&str) -> usize,
    pub keep_separator: bool,
    pub add_start_index: bool,
}

impl TextSplitter {
    /// LangChain-style constructor: `TextSplitter(chunk_size, chunk_overlap)`.
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        Self::with_length(chunk_size, chunk_overlap, fallback_len)
    }

    pub fn with_length(
        chunk_size: usize,
        chunk_overlap: usize,
        length_function: fn(&str) -> usize,
    ) -> Self {
        assert!(
            chunk_overlap <= chunk_size,
            "chunk_overlap ({chunk_overlap}) must be <= chunk_size ({chunk_size})"
        );
        Self {
            chunk_size,
            chunk_overlap,
            length_function,
            keep_separator: true,
            add_start_index: false,
        }
    }

    pub fn with_token_length(chunk_size: usize, chunk_overlap: usize) -> Self {
        Self::with_length(chunk_size, chunk_overlap, token_count)
    }

    pub(crate) fn length(&self, text: &str) -> usize {
        (self.length_function)(text)
    }

    /// Join helper shared by all merging implementations.
    pub(crate) fn merge(
        &self,
        splits: &[String],
        separator: &str,
    ) -> Vec<String> {
        let separator_len = self.length(separator);
        let mut docs = Vec::new();
        let mut current_doc: Vec<String> = Vec::new();
        let mut total = 0usize;

        for split in splits {
            let len = self.length(split);
            let join_len = if current_doc.is_empty() { 0 } else { separator_len };
            if total + len + join_len > self.chunk_size {
                if total > self.chunk_size {
                    tracing::warn!(
                        size = total,
                        chunk_size = self.chunk_size,
                        "created a chunk longer than chunk_size"
                    );
                }
                if !current_doc.is_empty() {
                    if let Some(doc) = join_docs(&current_doc, separator) {
                        docs.push(doc);
                    }
                    while !current_doc.is_empty()
                        && (total > self.chunk_overlap
                            || (total + len
                                + (if current_doc.is_empty() { 0 } else { separator_len })
                                > self.chunk_size
                                && total > 0))
                    {
                        total -= self.length(&current_doc[0])
                            + if current_doc.len() > 1 { separator_len } else { 0 };
                        current_doc.remove(0);
                    }
                }
            }
            current_doc.push(split.clone());
            total += len + if current_doc.is_empty() { 0 } else { separator_len };
        }

        if let Some(doc) = join_docs(&current_doc, separator) {
            docs.push(doc);
        }
        docs
    }
}

// ---------------------------------------------------------------------------
// TokenTextSplitter + split_text_on_tokens (port of langchain).
// ---------------------------------------------------------------------------

pub struct TokenTextSplitter {
    base: TextSplitter,
}

impl TokenTextSplitter {
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        Self {
            base: TextSplitter::with_token_length(chunk_size, chunk_overlap),
        }
    }

    pub fn split_text(&self, text: &str) -> Vec<String> {
        split_text_on_tokens(text, self.base.chunk_size, self.base.chunk_overlap)
    }
}

/// Faithful port of langchain `split_text_on_tokens`.
pub fn split_text_on_tokens(text: &str, chunk_size: usize, chunk_overlap: usize) -> Vec<String> {
    let tk = match tokenizer() {
        Some(t) => t,
        None => return chunk_text_fallback(text, chunk_size, chunk_overlap),
    };
    let input_ids: Vec<u32> = match tk.encode(text, false) {
        Ok(e) => e.get_ids().to_vec(),
        Err(_) => return chunk_text_fallback(text, chunk_size, chunk_overlap),
    };
    if input_ids.is_empty() {
        return vec![];
    }

    let max_length = chunk_size.saturating_sub(chunk_overlap).max(1);
    let mut splits = Vec::new();
    let mut start = 0usize;
    let mut cur = (start + max_length).min(input_ids.len());

    while start < input_ids.len() {
        while cur < input_ids.len() && input_ids[start..cur].len() <= max_length {
            cur += 1;
        }
        if cur == start {
            cur += 1;
        }
        let chunk_ids = &input_ids[start..cur];
        if let Ok(decoded) = tk.decode(chunk_ids, false) {
            if !decoded.trim().is_empty() {
                splits.push(decoded);
            }
        }
        start = cur;
    }
    splits
}

fn chunk_text_fallback(text: &str, chunk_size: usize, chunk_overlap: usize) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return vec![];
    }
    let sep = "\n\n";
    let splits: Vec<String> = text
        .split(sep)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    TextSplitter::with_length(chunk_size, chunk_overlap, fallback_len).merge(&splits, sep)
}

// ---------------------------------------------------------------------------
// Shared helpers.
// ---------------------------------------------------------------------------

pub(crate) fn find_separator<'a>(text: &str, separators: &'a [String]) -> (&'a String, &'a [String]) {
    for (i, s) in separators.iter().enumerate() {
        if s.is_empty() {
            return (s, &separators[i + 1..]);
        }
        if text.contains(s) {
            return (s, &separators[i + 1..]);
        }
    }
    let last = separators.len() - 1;
    (&separators[last], &[])
}

pub(crate) fn split_with_regex(text: &str, separator: &str, _keep_separator: bool) -> Vec<String> {
    if separator.is_empty() {
        return text.chars().map(|c| c.to_string()).collect();
    }
    text.split(separator)
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .collect()
}

pub(crate) fn join_docs(docs: &[String], separator: &str) -> Option<String> {
    if docs.is_empty() {
        return None;
    }
    let text = docs.join(separator);
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}
