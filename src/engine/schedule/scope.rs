//! Which downloads a schedule rule applies to.

use serde::{Deserialize, Deserializer, Serialize};

/// A batch a scope names; `label` is only for display.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchRef {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The downloads a queue window or speed entry applies to: any of the named
/// tags, any of the hosts and any of the batches, with every kind that has
/// names holding at once. A kind with no names puts no limit on that kind, so
/// an empty scope is every download. A name that matches nothing (any more)
/// simply never applies; it is kept.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize)]
pub struct Scope {
    /// Downloads carrying the tag, ignoring case.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Downloads whose URL has this host: an exact match ignoring case, with
    /// no port, no `www.` folding and no wildcards.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    /// Downloads added in the batch with this id.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub batches: Vec<BatchRef>,
}

/// The shape of the lists as saved: three optional lists.
#[derive(Deserialize)]
struct Lists {
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    hosts: Vec<String>,
    #[serde(default)]
    batches: Vec<BatchRef>,
}

/// The single scope older versions saved.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Single {
    All,
    Tag {
        tag: String,
    },
    Host {
        host: String,
    },
    Batch {
        id: u64,
        #[serde(default)]
        label: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Wire {
    // Tried first: a list-shaped scope has no `kind`, so it never matches.
    Single(Single),
    Lists(Lists),
}

impl<'de> Deserialize<'de> for Scope {
    /// Reads the three lists, or an older single scope as a list of one.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match Wire::deserialize(d)? {
            Wire::Single(Single::All) => Scope::default(),
            Wire::Single(Single::Tag { tag }) => Scope::tag(tag),
            Wire::Single(Single::Host { host }) => Scope::host(host),
            Wire::Single(Single::Batch { id, label }) => Scope {
                batches: vec![BatchRef { id, label }],
                ..Scope::default()
            },
            Wire::Lists(l) => Scope {
                tags: l.tags,
                hosts: l.hosts,
                batches: l.batches,
            },
        })
    }
}

/// What a scope looks at on one download.
#[derive(Debug, Clone, Copy)]
pub struct Subject<'a> {
    pub tags: &'a [String],
    pub url: &'a str,
    pub batch: Option<u64>,
}

impl Scope {
    /// Every download.
    pub fn all() -> Scope {
        Scope::default()
    }

    /// Downloads carrying `tag`.
    pub fn tag(tag: impl Into<String>) -> Scope {
        Scope {
            tags: vec![tag.into()],
            ..Scope::default()
        }
    }

    /// Downloads from `host`.
    pub fn host(host: impl Into<String>) -> Scope {
        Scope {
            hosts: vec![host.into()],
            ..Scope::default()
        }
    }

    /// Downloads in batch `id`.
    pub fn batch(id: u64) -> Scope {
        Scope {
            batches: vec![BatchRef { id, label: None }],
            ..Scope::default()
        }
    }

    /// Whether this is the all-downloads scope: it names nothing.
    pub fn is_all(&self) -> bool {
        self.tags.is_empty() && self.hosts.is_empty() && self.batches.is_empty()
    }

    /// Whether the rule applies to `subject`.
    pub fn matches(&self, subject: &Subject<'_>) -> bool {
        let tag_ok = self.tags.is_empty()
            || self.tags.iter().any(|want| {
                let want = want.trim().to_lowercase();
                subject.tags.iter().any(|t| t.to_lowercase() == want)
            });
        let batch_ok =
            self.batches.is_empty() || self.batches.iter().any(|b| subject.batch == Some(b.id));
        let host_ok = self.hosts.is_empty() || {
            let host = url::Url::parse(subject.url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned));
            host.is_some_and(|h| self.hosts.iter().any(|w| h.eq_ignore_ascii_case(w.trim())))
        };
        tag_ok && batch_ok && host_ok
    }

    /// This scope in a form two equal scopes share: names lower-cased,
    /// trimmed, sorted and deduplicated, the batch labels dropped. The key a
    /// shared limiter is kept under.
    pub(crate) fn canonical(&self) -> Scope {
        fn names(list: &[String]) -> Vec<String> {
            let mut out: Vec<String> = list.iter().map(|s| s.trim().to_lowercase()).collect();
            out.sort();
            out.dedup();
            out
        }
        let mut batches: Vec<u64> = self.batches.iter().map(|b| b.id).collect();
        batches.sort_unstable();
        batches.dedup();
        Scope {
            tags: names(&self.tags),
            hosts: names(&self.hosts),
            batches: batches
                .into_iter()
                .map(|id| BatchRef { id, label: None })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }

    fn subject<'a>(t: &'a [String], url: &'a str, batch: Option<u64>) -> Subject<'a> {
        Subject {
            tags: t,
            url,
            batch,
        }
    }

    #[test]
    fn matches_a_tag_a_host_and_a_batch() {
        let t = names(&["Course-2"]);
        let s = subject(&t, "https://Cdn.Example.com:8443/a/b.zip", Some(7));
        assert!(Scope::all().matches(&s));
        assert!(Scope::tag("course-2").matches(&s));
        assert!(!Scope::tag("course").matches(&s));
        assert!(Scope::host("cdn.example.com").matches(&s));
        assert!(!Scope::host("example.com").matches(&s));
        assert!(Scope::batch(7).matches(&s));
        assert!(!Scope::batch(8).matches(&s));
    }

    #[test]
    fn any_within_a_kind_and_all_kinds_together() {
        let t = names(&["b"]);
        let s = subject(&t, "https://cdn.example.com/f", Some(2));
        let two_tags = Scope {
            tags: names(&["a", "B"]),
            ..Scope::default()
        };
        assert!(two_tags.matches(&s));
        let both = Scope {
            tags: names(&["a", "b"]),
            hosts: names(&["cdn.example.com", "other.org"]),
            ..Scope::default()
        };
        assert!(both.matches(&s));
        let wrong_host = Scope {
            tags: names(&["b"]),
            hosts: names(&["other.org"]),
            ..Scope::default()
        };
        assert!(!wrong_host.matches(&s), "every named kind must hold");
        let batches = Scope {
            batches: vec![
                BatchRef { id: 1, label: None },
                BatchRef { id: 2, label: None },
            ],
            ..Scope::default()
        };
        assert!(batches.matches(&s));
    }

    #[test]
    fn an_unparseable_url_matches_no_host() {
        let s = subject(&[], "not a url", None);
        assert!(!Scope::host("not a url").matches(&s));
    }

    #[test]
    fn saves_three_lists_and_leaves_empty_ones_out() {
        assert_eq!(serde_json::to_string(&Scope::all()).unwrap(), "{}");
        assert_eq!(
            serde_json::to_string(&Scope::tag("x")).unwrap(),
            r#"{"tags":["x"]}"#
        );
        let s = Scope {
            tags: names(&["a", "b"]),
            hosts: names(&["h.org"]),
            batches: vec![BatchRef {
                id: 3,
                label: Some("Course".into()),
            }],
        };
        let back: Scope = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn an_old_single_scope_loads_as_a_list_of_one() {
        let load = |j: &str| serde_json::from_str::<Scope>(j).unwrap();
        assert_eq!(load(r#"{"kind":"all"}"#), Scope::all());
        assert_eq!(load(r#"{"kind":"tag","tag":"x"}"#), Scope::tag("x"));
        assert_eq!(
            load(r#"{"kind":"host","host":"h.org"}"#),
            Scope::host("h.org")
        );
        assert_eq!(
            load(r#"{"kind":"batch","id":4,"label":"Course"}"#).batches,
            vec![BatchRef {
                id: 4,
                label: Some("Course".into())
            }]
        );
        assert_eq!(load(r#"{"kind":"batch","id":4}"#), Scope::batch(4));
    }

    #[test]
    fn equal_scopes_share_a_key_whatever_their_order_case_or_labels() {
        let a = Scope {
            tags: names(&[" B ", "a", "A"]),
            hosts: names(&["H.org"]),
            batches: vec![BatchRef {
                id: 2,
                label: Some("x".into()),
            }],
        };
        let b = Scope {
            tags: names(&["a", "b"]),
            hosts: names(&["h.org"]),
            batches: vec![BatchRef { id: 2, label: None }],
        };
        assert_eq!(a.canonical(), b.canonical());
    }
}
