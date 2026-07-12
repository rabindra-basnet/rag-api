use crate::service::chunker_service::character::RecursiveCharacterTextSplitter;

/// Markdown-aware recursive splitter (uses markdown-specific separators).
pub struct MarkdownTextSplitter {
    inner: RecursiveCharacterTextSplitter,
}

impl MarkdownTextSplitter {
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        Self {
            inner: RecursiveCharacterTextSplitter::with_separators(
                chunk_size,
                chunk_overlap,
                vec![
                    "\n#{1,6} ".to_string(),
                    "\n```".to_string(),
                    "\n---+".to_string(),
                    "\n___+".to_string(),
                    "\n\n".to_string(),
                    "\n".to_string(),
                    " ".to_string(),
                    "".to_string(),
                ],
                false,
            ),
        }
    }

    pub fn split_text(&self, text: &str) -> Vec<String> {
        self.inner.split_text(text)
    }
}

/// Port of langchain `MarkdownHeaderTextSplitter`: splits on markdown headers
/// and keeps the header metadata attached to each content chunk.
#[derive(Clone, Debug)]
pub struct HeaderType {
    pub level: usize,
    pub name: String,
}

pub struct MarkdownHeaderTextSplitter {
    headers_to_split_on: Vec<HeaderType>,
    return_each_line: bool,
    strip_headers: bool,
}

impl MarkdownHeaderTextSplitter {
    pub fn new(headers_to_split_on: Vec<HeaderType>) -> Self {
        Self {
            headers_to_split_on,
            return_each_line: false,
            strip_headers: false,
        }
    }

    pub fn with_options(mut self, return_each_line: bool, strip_headers: bool) -> Self {
        self.return_each_line = return_each_line;
        self.strip_headers = strip_headers;
        self
    }

    /// Returns `(content, metadata)` pairs, mirroring langchain's output.
    pub fn split_text(&self, text: &str) -> Vec<(String, std::collections::HashMap<String, String>)> {
        let mut lines = text.lines().peekable();
        let mut results: Vec<(String, std::collections::HashMap<String, String>)> = Vec::new();
        let mut current_content: Vec<String> = Vec::new();
        let mut current_metadata: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        while let Some(line) = lines.next() {
            let mut matched: Option<&HeaderType> = None;
            for h in &self.headers_to_split_on {
                let prefix = "#".repeat(h.level);
                if line.starts_with(&format!("{prefix} ")) || line == prefix {
                    matched = Some(h);
                    break;
                }
            }

            if let Some(h) = matched {
                let value = line
                    .trim_start_matches(&"#".repeat(h.level))
                    .trim_start_matches(' ')
                    .trim()
                    .to_string();
                // Close the current block.
                if !current_content.is_empty() {
                    results.push((
                        std::mem::take(&mut current_content).join("\n"),
                        current_metadata.clone(),
                    ));
                }
                current_metadata.insert(h.name.clone(), value);
                if !self.strip_headers {
                    current_content.push(line.to_string());
                }
            } else {
                current_content.push(line.to_string());
            }
        }

        if !current_content.is_empty() {
            results.push((
                std::mem::take(&mut current_content).join("\n"),
                current_metadata,
            ));
        }

        results
            .into_iter()
            .filter(|(c, _)| !c.trim().is_empty())
            .collect()
    }
}
