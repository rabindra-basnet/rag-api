use crate::service::chunker_service::base::Language;
use crate::service::chunker_service::character::RecursiveCharacterTextSplitter;

/// All language-based splitters are `RecursiveCharacterTextSplitter` instances
/// configured with that language's separator set (port of langchain's
/// `PythonCodeTextSplitter`, `JSFrameworkTextSplitter`, `LatexTextSplitter`,
/// `MarkdownTextSplitter`, `RustCodeTextSplitter`, `HtmlTextSplitter`, ...).

macro_rules! lang_splitter {
    ($name:ident, $lang:expr) => {
        pub struct $name {
            inner: RecursiveCharacterTextSplitter,
        }

        impl $name {
            pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
                Self {
                    inner: RecursiveCharacterTextSplitter::with_separators(
                        chunk_size,
                        chunk_overlap,
                        $lang.separators(),
                        false,
                    ),
                }
            }

            pub fn split_text(&self, text: &str) -> Vec<String> {
                self.inner.split_text(text)
            }
        }
    };
}

lang_splitter!(PythonTextSplitter, Language::Python);
lang_splitter!(JSTextSplitter, Language::Js);
lang_splitter!(LatexTextSplitter, Language::Latex);
lang_splitter!(MarkdownTextSplitter, Language::Markdown);
lang_splitter!(RustTextSplitter, Language::Rust);
lang_splitter!(HtmlTextSplitter, Language::Html);
lang_splitter!(CppTextSplitter, Language::Cpp);
lang_splitter!(GoTextSplitter, Language::Go);
lang_splitter!(JavaTextSplitter, Language::Java);
lang_splitter!(PhpTextSplitter, Language::Php);
lang_splitter!(RubyTextSplitter, Language::Ruby);
lang_splitter!(SwiftTextSplitter, Language::Swift);
lang_splitter!(ScalaTextSplitter, Language::Scala);
lang_splitter!(SolTextSplitter, Language::Sol);
lang_splitter!(ProtoTextSplitter, Language::Proto);
lang_splitter!(RstTextSplitter, Language::Rst);

/// Build a recursive splitter for an arbitrary language.
pub fn recursive_for_language(lang: Language, chunk_size: usize, chunk_overlap: usize) -> RecursiveCharacterTextSplitter {
    RecursiveCharacterTextSplitter::with_separators(chunk_size, chunk_overlap, lang.separators(), false)
}
