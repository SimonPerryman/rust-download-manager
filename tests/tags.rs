//! Tags: [`Manager::add_tags`], [`Manager::remove_tags`] and
//! [`AddOptions::tags`] keep user labels on a download's record under the
//! tag rules (trimmed, 1 to 32 characters, unique ignoring case, at most 16).

mod common;

use std::path::Path;

use rdm::engine::tags::{MAX_TAGS, MAX_TAG_LEN};
use rdm::{AddOptions, Error, JsonStore, Manager, Store};

/// Build a manager over the JSON store `dir/state.json`.
fn manager_in(dir: &Path) -> Manager {
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.join("state.json")));
    Manager::load(store, tokio::runtime::Handle::current()).unwrap()
}

fn strings(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|t| t.to_string()).collect()
}

fn tags_of(mgr: &Manager, id: u64) -> Vec<String> {
    mgr.snapshot()
        .into_iter()
        .find(|s| s.id == id)
        .unwrap()
        .tags
}

/// A download stopped at once: tags don't depend on status.
fn idle(mgr: &Manager, dir: &Path) -> u64 {
    let id = mgr.add("http://127.0.0.1:9/never", dir, None, None);
    mgr.stop(id, false);
    id
}

/// AC: case-insensitive duplicates are ignored and order is kept.
#[tokio::test]
async fn add_tags_skips_duplicates_ignoring_case_and_keeps_order() {
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let id = idle(&mgr, dir.path());

    assert_eq!(
        mgr.add_tags(&[id], &strings(&["work", " iso "])).unwrap(),
        1
    );
    assert_eq!(
        mgr.add_tags(&[id], &strings(&["WORK", "course-2", "Course-2"]))
            .unwrap(),
        1
    );
    assert_eq!(tags_of(&mgr, id), ["work", "iso", "course-2"]);
    assert_eq!(
        mgr.add_tags(&[id], &strings(&["ISO"])).unwrap(),
        0,
        "nothing new"
    );
}

/// AC: an empty, a 33-character and a 17th tag are each rejected with
/// `InvalidRequest`, and a rejected call changes nothing.
#[tokio::test]
async fn invalid_tags_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let a = idle(&mgr, dir.path());
    let b = idle(&mgr, dir.path());

    let rejects = |tags: Vec<String>| {
        let err = mgr.add_tags(&[a], &tags).unwrap_err();
        assert!(matches!(err, Error::InvalidRequest(_)), "{err:?}");
    };
    rejects(strings(&[""]));
    rejects(strings(&["   "]));
    rejects(vec!["x".repeat(MAX_TAG_LEN + 1)]);
    assert!(tags_of(&mgr, a).is_empty());
    mgr.add_tags(&[a], &["x".repeat(MAX_TAG_LEN)]).unwrap();

    let sixteen: Vec<String> = (1..MAX_TAGS).map(|i| format!("t{i}")).collect();
    mgr.add_tags(&[a], &sixteen).unwrap();
    assert_eq!(tags_of(&mgr, a).len(), MAX_TAGS);
    rejects(strings(&["seventeenth"]));
    assert_eq!(tags_of(&mgr, a).len(), MAX_TAGS);

    // All or nothing across downloads: `b` has room, `a` doesn't.
    let err = mgr.add_tags(&[b, a], &strings(&["more"])).unwrap_err();
    assert!(matches!(err, Error::InvalidRequest(_)), "{err:?}");
    assert!(tags_of(&mgr, b).is_empty());
}

/// `remove_tags` matches ignoring case and leaves the rest in order.
#[tokio::test]
async fn remove_tags_matches_ignoring_case() {
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let a = idle(&mgr, dir.path());
    let b = idle(&mgr, dir.path());
    mgr.add_tags(&[a, b], &strings(&["work", "iso", "keep"]))
        .unwrap();

    assert_eq!(mgr.remove_tags(&[a, b], &strings(&[" WORK", "ISO"])), 2);
    assert_eq!(tags_of(&mgr, a), ["keep"]);
    assert_eq!(tags_of(&mgr, b), ["keep"]);
    assert_eq!(mgr.remove_tags(&[a], &strings(&["absent"])), 0);
}

/// A repeated id counts once.
#[tokio::test]
async fn repeated_ids_count_once() {
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());
    let a = idle(&mgr, dir.path());

    assert_eq!(mgr.add_tags(&[a, a], &strings(&["work"])).unwrap(), 1);
    assert_eq!(tags_of(&mgr, a), ["work"]);
    assert_eq!(mgr.remove_tags(&[a, a], &strings(&["work"])), 1);
}

/// AC: `add_with` stores the given tags under the same rules, and rejects
/// invalid ones without adding anything.
#[tokio::test]
async fn add_with_stores_tags() {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = manager_in(dir.path());

    let options = |tags: &[&str]| AddOptions {
        tags: strings(tags),
        ..AddOptions::default()
    };
    let id = mgr
        .add_with(
            server.url("/file"),
            dir.path(),
            options(&["work", "Work", "iso"]),
        )
        .unwrap();
    assert_eq!(tags_of(&mgr, id), ["work", "iso"]);

    let err = mgr
        .add_with(server.url("/file"), dir.path(), options(&[""]))
        .unwrap_err();
    assert!(matches!(err, Error::InvalidRequest(_)), "{err:?}");
    let too_many: Vec<String> = (0..=MAX_TAGS).map(|i| format!("t{i}")).collect();
    let refs: Vec<&str> = too_many.iter().map(String::as_str).collect();
    assert!(mgr
        .add_with(server.url("/file"), dir.path(), options(&refs))
        .is_err());
    assert_eq!(mgr.snapshot().len(), 1, "a rejected add adds nothing");

    let stored = JsonStore::new(dir.path().join("state.json"))
        .load()
        .unwrap();
    assert_eq!(stored.downloads[0].tags, ["work", "iso"]);
}
