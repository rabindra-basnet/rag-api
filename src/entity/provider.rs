use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Credential,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::Credential => "credential",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
