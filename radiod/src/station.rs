use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StationUri {
    Orbox { country: String, alias: String },
    Direct { url: String },
}

#[derive(Debug)]
pub enum StationUriParseError {
    UnknownScheme(String),
    InvalidFormat(String),
}

impl fmt::Display for StationUriParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StationUriParseError::UnknownScheme(scheme) => {
                write!(
                    f,
                    "unknown URI scheme '{}', expected 'orbox:' or 'direct:'",
                    scheme
                )
            }
            StationUriParseError::InvalidFormat(msg) => {
                write!(f, "invalid URI format: {}", msg)
            }
        }
    }
}

impl std::error::Error for StationUriParseError {}

impl FromStr for StationUri {
    type Err = StationUriParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(rest) = s.strip_prefix("orbox:") {
            let rest = rest.trim_start_matches("//");
            let (country, alias) = rest.split_once('/').ok_or_else(|| {
                StationUriParseError::InvalidFormat(format!(
                    "expected 'orbox:<country>/<alias>', got '{}'",
                    s
                ))
            })?;
            let country = country.trim().to_string();
            let alias = alias.trim().to_string();
            if country.is_empty() || alias.is_empty() {
                return Err(StationUriParseError::InvalidFormat(format!(
                    "country and alias must not be empty in '{}'",
                    s
                )));
            }
            Ok(StationUri::Orbox { country, alias })
        } else if let Some(rest) = s.strip_prefix("direct:") {
            let url = rest.trim_start_matches("//").to_string();
            if url.is_empty() {
                return Err(StationUriParseError::InvalidFormat(
                    "direct URI must include a URL".into(),
                ));
            }
            Ok(StationUri::Direct { url })
        } else {
            let scheme = s.split(':').next().unwrap_or(s);
            Err(StationUriParseError::UnknownScheme(scheme.to_string()))
        }
    }
}

impl fmt::Display for StationUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StationUri::Orbox { country, alias } => write!(f, "orbox:{}/{}", country, alias),
            StationUri::Direct { url } => write!(f, "direct:{}", url),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_orbox_uri() {
        let uri: StationUri = "orbox:uk/bbcradio1".parse().unwrap();
        assert_eq!(
            uri,
            StationUri::Orbox {
                country: "uk".into(),
                alias: "bbcradio1".into()
            }
        );
        assert_eq!(uri.to_string(), "orbox:uk/bbcradio1");
    }

    #[test]
    fn parse_direct_uri() {
        let uri: StationUri = "direct:https://example.com/stream".parse().unwrap();
        assert_eq!(
            uri,
            StationUri::Direct {
                url: "https://example.com/stream".into()
            }
        );
        assert_eq!(uri.to_string(), "direct:https://example.com/stream");
    }

    #[test]
    fn reject_unknown_scheme() {
        let result: Result<StationUri, _> = "http://example.com".parse();
        assert!(result.is_err());
    }

    #[test]
    fn reject_empty_orbox() {
        let result: Result<StationUri, _> = "orbox:/".parse();
        assert!(result.is_err());
    }
}
