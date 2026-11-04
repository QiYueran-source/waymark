//! Deterministic fixtures for tests and for adapters that need reproducible
//! output.
//!
//! Everything here is public so integration tests in `tests/` can use it, which
//! is why it ships in the library rather than behind `#[cfg(test)]`.

use chrono::{TimeZone, Utc};

use crate::id::{IdSource, WorkItemId};
use crate::model::{CategoryKey, FieldValue, Status, WorkItem};
use crate::store::Project;
use crate::time::{Clock, FixedClock, Timestamp};

/// The instant every fixture uses unless a test moves the clock.
pub const FIXTURE_INSTANT: &str = "2026-11-03T05:43:10Z";

/// An ID source that yields a deterministic sequence of bodies.
///
/// The sequence is predictable on purpose: a test that asserts "the second item
/// got a different ID" needs the source to be reproducible, not random.
#[derive(Debug, Clone)]
pub struct SequentialIds {
    next: u64,
    width: usize,
}

impl SequentialIds {
    /// A source whose first body is `000000000000`, then `000000000001`, and so
    /// on through the Crockford Base32 alphabet.
    pub fn new() -> Self {
        Self {
            next: 0,
            width: crate::id::BODY_LEN,
        }
    }

    /// A source that starts at `start`.
    pub fn starting_at(start: u64) -> Self {
        Self {
            next: start,
            width: crate::id::BODY_LEN,
        }
    }
}

impl Default for SequentialIds {
    fn default() -> Self {
        Self::new()
    }
}

impl IdSource for SequentialIds {
    fn next_candidate(&mut self) -> String {
        let body = encode_crockford(self.next, self.width);
        self.next += 1;
        body
    }
}

/// An ID source that always returns the same body, so a test can force the
/// collision path and the `ID_GENERATION_FAILED` path.
#[derive(Debug, Clone)]
pub struct ConstantIds {
    body: String,
}

impl ConstantIds {
    /// Always yield `body`.
    pub fn new(body: impl Into<String>) -> Self {
        Self { body: body.into() }
    }
}

impl IdSource for ConstantIds {
    fn next_candidate(&mut self) -> String {
        self.body.clone()
    }
}

fn encode_crockford(mut value: u64, width: usize) -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
    let mut buffer = vec![b'0'; width];
    for slot in buffer.iter_mut().rev() {
        *slot = ALPHABET[(value % 32) as usize];
        value /= 32;
    }
    String::from_utf8(buffer).expect("the alphabet is ASCII")
}

/// The instant fixtures use.
pub fn instant() -> Timestamp {
    Timestamp::parse(FIXTURE_INSTANT).expect("the fixture instant is valid")
}

/// A clock frozen at [`FIXTURE_INSTANT`].
pub fn clock() -> FixedClock {
    FixedClock::new(
        Utc.with_ymd_and_hms(2026, 11, 3, 5, 43, 10)
            .single()
            .expect("a valid instant"),
    )
}

/// A builder for a valid work item, so a test names only what it cares about.
#[derive(Debug, Clone)]
pub struct WorkItemBuilder {
    item: WorkItem,
}

impl WorkItemBuilder {
    /// Override the title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.item.title = title.into();
        self
    }

    /// Override the category key.
    pub fn with_category(mut self, category: &str) -> Self {
        self.item.category = CategoryKey::parse(category).expect("a valid category key");
        self
    }

    /// Override the owner.
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.item.owner = owner.into();
        self
    }

    /// Override the status.
    pub fn with_status(mut self, status: Status) -> Self {
        self.item.status = status;
        self
    }

    /// Set the body.
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.item.body = body.into();
        self
    }

    /// Add one custom field.
    pub fn with_field(mut self, key: &str, value: FieldValue) -> Self {
        self.item.custom_fields.insert(key.to_owned(), value);
        self
    }

    /// Finish the item.
    pub fn build(self) -> WorkItem {
        self.item
    }
}

/// A fully populated, valid work item for `id`, `category`, and `title`.
pub fn work_item(id: &str, category: &str, title: &str) -> WorkItemBuilder {
    WorkItemBuilder {
        item: WorkItem {
            id: WorkItemId::parse(id).expect("a valid stable ID"),
            title: title.to_owned(),
            category: CategoryKey::parse(category).expect("a valid category key"),
            status: Status::Open,
            owner: "frank".to_owned(),
            creator: None,
            created_at: instant(),
            updated_at: instant(),
            closed_at: None,
            parent: None,
            dependencies: Vec::new(),
            soft_links: Vec::new(),
            custom_fields: Default::default(),
            archived_at: None,
            archive_reason: None,
            schema: crate::model::FORMAT_GENERATION,
            body: String::new(),
        },
    }
}

/// The smallest valid work item for `id`: one category, one owner, no optional
/// fields, no body.
pub fn minimal_item(id: &str) -> WorkItem {
    work_item(id, "bug", "A title").build()
}

/// A project in a fresh temporary directory, for store and operation tests.
///
/// The returned `TempDir` must be kept alive for the project to stay on disk.
pub fn project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf())
        .expect("the temporary directory path is UTF-8");
    let project = Project::init(&root, crate::store::InitOptions::default())
        .expect("a fresh project initialises");
    (dir, project)
}

/// A project whose clock is frozen and whose IDs are deterministic.
pub fn deterministic_project() -> (tempfile::TempDir, Project) {
    let (dir, project) = project();
    let project = project
        .with_clock(std::sync::Arc::new(clock()))
        .with_id_source(Box::new(SequentialIds::new()));
    (dir, project)
}

/// A frozen clock as a trait object.
pub fn shared_clock() -> std::sync::Arc<dyn Clock> {
    std::sync::Arc::new(clock())
}

/// The fixtures, grouped so a test can read as `fixtures::work_item(…)`.
pub mod fixtures {
    pub use super::{
        clock, deterministic_project, instant, minimal_item, project, shared_clock, work_item,
        WorkItemBuilder,
    };
}
