/// Split text into overlapping chunks, preferring paragraph then sentence
/// boundaries, sized in characters (approximate token proxy).
pub fn chunk_text(text: &str, max_chars: usize, overlap_chars: usize) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return vec![];
    }
    if text.len() <= max_chars {
        return vec![text.to_string()];
    }

    // Split into paragraphs, then pack them into chunks.
    let mut units: Vec<&str> = Vec::new();
    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        if para.len() <= max_chars {
            units.push(para);
        } else {
            // Paragraph too big: split on sentence-ish boundaries.
            let mut start = 0;
            let bytes = para.as_bytes();
            let mut last_cut = 0;
            for (i, &b) in bytes.iter().enumerate() {
                if matches!(b, b'.' | b'!' | b'?' | b'\n') {
                    last_cut = i + 1;
                }
                if i - start >= max_chars {
                    let cut = if last_cut > start { last_cut } else { i };
                    // Ensure we cut on a char boundary.
                    let mut cut = cut.min(para.len());
                    while cut < para.len() && !para.is_char_boundary(cut) {
                        cut += 1;
                    }
                    units.push(para[start..cut].trim());
                    start = cut;
                    last_cut = start;
                }
            }
            if start < para.len() {
                units.push(para[start..].trim());
            }
        }
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for unit in units {
        if !current.is_empty() && current.len() + unit.len() + 2 > max_chars {
            // Carry a tail of the previous chunk as overlap.
            let tail = overlap_tail(&current, overlap_chars);
            chunks.push(std::mem::take(&mut current));
            current = tail;
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(unit);
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    chunks
}

fn overlap_tail(s: &str, overlap: usize) -> String {
    if overlap == 0 || s.len() <= overlap {
        return String::new();
    }
    let mut idx = s.len() - overlap;
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    s[idx..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_single_chunk() {
        assert_eq!(chunk_text("hello world", 100, 20), vec!["hello world"]);
    }

    #[test]
    fn long_text_multiple_chunks() {
        let text = "para one. ".repeat(50) + "\n\n" + &"para two. ".repeat(50);
        let chunks = chunk_text(&text, 300, 50);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.len() <= 400));
    }

    #[test]
    fn empty_text() {
        assert!(chunk_text("   ", 100, 20).is_empty());
    }
}
