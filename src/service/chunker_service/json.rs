use serde_json::Value;

/// Port of langchain `RecursiveJsonSplitter`: recursively walks a JSON document
/// and flattens it into chunks, keeping nested keys as path metadata.
pub struct RecursiveJsonSplitter {
    max_chunk_size: usize,
}

impl RecursiveJsonSplitter {
    pub fn new(max_chunk_size: usize) -> Self {
        Self { max_chunk_size }
    }

    pub fn split_text(&self, text: &str) -> Vec<String> {
        match serde_json::from_str::<Value>(text) {
            Ok(v) => {
                let mut out = Vec::new();
                self._split_json(&v, &mut vec![], &mut out);
                out
            }
            Err(_) => vec![text.to_string()],
        }
    }

    fn _split_json(&self, value: &Value, path: &mut Vec<String>, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    path.push(k.clone());
                    self._split_json(v, path, out);
                    path.pop();
                }
            }
            Value::Array(arr) => {
                for item in arr {
                    self._split_json(item, path, out);
                }
            }
            _ => {
                let leaf = if path.is_empty() {
                    value.to_string()
                } else {
                    let key = path.join(".");
                    format!("\"{key}\": {value}")
                };
                if out.is_empty()
                    || out
                        .last()
                        .map(|c| c.chars().count() + leaf.chars().count() + 1 > self.max_chunk_size)
                        .unwrap_or(true)
                {
                    out.push(leaf);
                } else if let Some(last) = out.last_mut() {
                    last.push_str(",\n");
                    last.push_str(&leaf);
                }
            }
        }
    }
}
