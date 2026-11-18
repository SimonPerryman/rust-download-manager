//! Which downloads a schedule rule applies to.

use serde::{Deserialize, Serialize};

/// The downloads a queue window or speed entry applies to. A rule whose tag,
/// host or batch matches nothing (any more) simply never applies; it is kept.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scope {
    /// Every download.
    #[default]
    All,
    /// Downloads carrying the tag, ignoring case.
    Tag { tag: String },
    /// Downloads whose URL has this host: an exact match ignoring case, with
    /// no port, no `www.` folding and no wildcards.
    Host { host: String },
    /// Downloads added in the batch with this id; `label` is only for display.
    Batch {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}

/// What a scope looks at on one download.
#[derive(Debug, Clone, Copy)]
pub struct Subject<'a> {
    pub tags: &'a [String],
    pub url: &'a str,
    pub batch: Option<u64>,
}

impl Scope {
    /// Whether this is the all-downloads scope.
    pub fn is_all(&self) -> bool {
        matches!(self, Scope::All)
    }

    /// Whether the rule applies to `subject`.
    pub fn matches(&self, subject: &Subject<'_>) -> bool {
        match self {
            Scope::All => true,
            Scope::Tag { tag } => {
                let tag = tag.trim().to_lowercase();
                subject.tags.iter().any(|t| t.to_lowercase() == tag)
            }
            Scope::Host { host } => url::Url::parse(subject.url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.eq_ignore_ascii_case(host.trim())))
                .unwrap_or(false),
            Scope::Batch { id, .. } => subject.batch == Some(*id),
        }
    }

    /// This scope in a form two equal scopes share: tag and host lower-cased
    /// and trimmed, the batch label dropped. The key a shared limiter is kept
    /// under.
    pub(crate) fn canonical(&self) -> Scope {
        match self {
            Scope::All => Scope::All,
            Scope::Tag { tag } => Scope::Tag {
                tag: tag.trim().to_lowercase(),
            },
            Scope::Host { host } => Scope::Host {
                host: host.trim().to_lowercase(),
            },
            Scope::Batch { id, .. } => Scope::Batch {
                id: *id,
                label: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn matches_a_tag_a_host_and_a_batch() {
        let t = tags(&["Course-2"]);
        let s = Subject {
            tags: &t,
            url: "https://Cdn.Example.com:8443/a/b.zip",
            batch: Some(7),
        };
        assert!(Scope::All.matches(&s));
        assert!(Scope::Tag {
            tag: "course-2".into()
        }
        .matches(&s));
        assert!(!Scope::Tag {
            tag: "course".into()
        }
        .matches(&s));
        assert!(Scope::Host {
            host: "cdn.example.com".into()
        }
        .matches(&s));
        assert!(!Scope::Host {
            host: "example.com".into()
        }
        .matches(&s));
        assert!(Scope::Batch { id: 7, label: None }.matches(&s));
        assert!(!Scope::Batch { id: 8, label: None }.matches(&s));
    }

    #[test]
    fn an_unparseable_url_matches_no_host() {
        let s = Subject {
            tags: &[],
            url: "not a url",
            batch: None,
        };
        assert!(!Scope::Host {
            host: "not a url".into()
        }
        .matches(&s));
    }

    #[test]
    fn the_wire_form_is_tagged_and_all_is_the_default() {
        let json = serde_json::to_string(&Scope::Tag { tag: "x".into() }).unwrap();
        assert_eq!(json, r#"{"kind":"tag","tag":"x"}"#);
        assert_eq!(Scope::default(), Scope::All);
    }
}
