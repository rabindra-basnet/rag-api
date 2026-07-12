use scraper::{Html, Selector};
use std::collections::HashMap;

/// Port of langchain `HTMLHeaderTextSplitter`: extracts header/section structure
/// from HTML and returns `(content, metadata)` chunks.
#[derive(Clone, Debug)]
pub struct HeaderType {
    pub selector: String,
    pub name: String,
}

pub struct HTMLHeaderTextSplitter {
    headers_to_split_on: Vec<HeaderType>,
    return_each_element: bool,
}

impl HTMLHeaderTextSplitter {
    pub fn new(headers_to_split_on: Vec<HeaderType>) -> Self {
        Self {
            headers_to_split_on,
            return_each_element: false,
        }
    }

    pub fn split_text(&self, text: &str) -> Vec<(String, HashMap<String, String>)> {
        let document = Html::parse_document(text);
        let mut results: Vec<(String, HashMap<String, String>)> = Vec::new();
        let mut current_metadata: HashMap<String, String> = HashMap::new();

        for header in &self.headers_to_split_on {
            if let Ok(selector) = Selector::parse(&header.selector) {
                for element in document.select(&selector) {
                    let content = if self.return_each_element {
                        element.text().collect::<Vec<_>>().join(" ").trim().to_string()
                    } else {
                        element.inner_html().trim().to_string()
                    };
                    if !content.is_empty() {
                        current_metadata.insert(header.name.clone(), content.clone());
                        results.push((content, current_metadata.clone()));
                    }
                }
            }
        }

        if results.is_empty() {
            let body = document
                .root_element()
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string();
            if !body.is_empty() {
                results.push((body, HashMap::new()));
            }
        }
        results
    }
}

/// Port of langchain `HTMLSectionSplitter`: splits an HTML document into
/// top-level sections delimited by the configured selectors.
pub struct HTMLSectionSplitter {
    selectors: Vec<HeaderType>,
}

impl HTMLSectionSplitter {
    pub fn new(selectors: Vec<HeaderType>) -> Self {
        Self { selectors }
    }

    pub fn split_text(&self, text: &str) -> Vec<(String, HashMap<String, String>)> {
        let document = Html::parse_document(text);
        let mut results: Vec<(String, HashMap<String, String>)> = Vec::new();

        for section in &self.selectors {
            if let Ok(selector) = Selector::parse(&section.selector) {
                for element in document.select(&selector) {
                    let content = element.inner_html().trim().to_string();
                    let metadata = {
                        let mut m = HashMap::new();
                        m.insert(section.name.clone(), element.value().name().to_string());
                        m
                    };
                    if !content.is_empty() {
                        results.push((content, metadata));
                    }
                }
            }
        }
        results
    }
}
