//! Parse a work-item document back into a [`WorkItem`].
//!
//! The parser is the inverse of [`crate::serialize`]. The pipeline is
//!
//! 1. locate the `---` … `---` fences (LF and CRLF tolerated);
//! 2. refuse YAML anchors/aliases before the YAML parser can expand them;
//! 3. read the frontmatter as an ordered key/value list, so an undeclared or
//!    duplicated key is a locatable issue rather than data silently dropped or
//!    overwritten;
//! 4. validate every field whose grammar is a domain rule;
//! 5. canonicalize the body.
//!
//! # Which code
//!
//! The registry distinguishes *cannot be read* from *read but invalid*:
//! malformed YAML, a missing fence, or invalid UTF-8 is `PARSE_ERROR`; a
//! well-formed document whose values break a rule is `VALIDATION_ERROR` with a
//! per-field issue list, which is what the spec pins for undeclared keys and
//! invalid value shapes. A document that declares a generation this release
//! cannot read is `UNSUPPORTED_VERSION`.
//!
//! A hand-edited spelling is accepted (`dependencies: []` parses as empty), but
//! writes always re-emit the one canonical form.

use camino::Utf8Path;
use serde_norway::Value;

use crate::error::{ValidationIssue, WaymarkError};
use crate::id::WorkItemId;
use crate::location::Location;
use crate::model::{
    CategoryKey, CustomFields, FieldKey, FieldValue, Status, WorkItem, FORMAT_GENERATION,
};
use crate::serialize::canonical_body;
use crate::time::{parse_utc, Timestamp};

/// The frontmatter keys a v1 document may carry, in the canonical writer's
/// order.
pub const FRONTMATTER_KEYS: &[&str] = &[
    "schema",
    "id",
    "title",
    "category",
    "status",
    "owner",
    "creator",
    "created_at",
    "updated_at",
    "closed_at",
    "parent",
    "dependencies",
    "soft_links",
    "custom_fields",
    "archived_at",
    "archive_reason",
];

/// The required keys, always present in a canonical document.
pub const REQUIRED_KEYS: &[&str] = &[
    "schema",
    "id",
    "title",
    "category",
    "status",
    "owner",
    "created_at",
    "updated_at",
];

/// Parse a complete document.
///
/// Accepts any spelling the frontmatter grammar allows, including the
/// `[]`/`{}` forms a hand edit may introduce. `path` is used for error context
/// and for the file name / ID agreement check.
pub fn parse_document(bytes: &[u8], path: &Utf8Path) -> Result<WorkItem, WaymarkError> {
    let (frontmatter_bytes, body_bytes) = split_frontmatter(bytes, path)?;
    if contains_yaml_anchor_or_alias(frontmatter_bytes) {
        return Err(WaymarkError::parse(
            path.to_string(),
            "YAML anchors and aliases are not allowed in frontmatter",
        ));
    }

    let value: Value =
        serde_norway::from_slice(frontmatter_bytes).map_err(|source| yaml_error(path, &source))?;
    let pairs = mapping_pairs(value, path)?;

    let body = String::from_utf8(body_bytes.to_vec()).map_err(|_| {
        WaymarkError::parse(path.to_string(), "the document body is not valid UTF-8")
    })?;

    reject_unreadable_generation(&pairs, path)?;

    Frontmatter::new(pairs, path).into_work_item(&body)
}

/// Read the frontmatter mapping as an ordered key/value list.
///
/// Order is preserved so a document that repeats a key is diagnosed against the
/// key that actually appears, and so a non-string key is reported rather than
/// coerced.
fn mapping_pairs(value: Value, path: &Utf8Path) -> Result<Vec<(String, Value)>, WaymarkError> {
    let mapping = match value {
        Value::Mapping(mapping) => mapping,
        Value::Null => {
            return Err(WaymarkError::parse(
                path.to_string(),
                "the frontmatter is empty",
            ));
        }
        _ => {
            return Err(WaymarkError::parse(
                path.to_string(),
                "the frontmatter must be a YAML mapping",
            ));
        }
    };
    let mut pairs = Vec::with_capacity(mapping.len());
    for (key, value) in mapping {
        match key {
            Value::String(key) => pairs.push((key, value)),
            other => {
                return Err(WaymarkError::parse(
                    path.to_string(),
                    format!("the frontmatter key {other:?} is not a string"),
                ));
            }
        }
    }
    Ok(pairs)
}

/// Refuse a document that declares a format generation this release cannot read.
///
/// This is checked before any other validation and reported as `PARSE_ERROR`
/// naming the generation: the registry's dedicated `UNSUPPORTED_VERSION` code
/// arrives with the migration ticket, and reporting a readable-but-unsupported
/// document as a field-level validation failure would send the caller looking
/// for a typo that is not there.
fn reject_unreadable_generation(
    pairs: &[(String, Value)],
    path: &Utf8Path,
) -> Result<(), WaymarkError> {
    let Some((_, value)) = pairs.iter().find(|(key, _)| key == "schema") else {
        return Ok(());
    };
    let Some(generation) = value.as_u64() else {
        return Ok(()); // the field validator reports a non-integer generation
    };
    if generation == u64::from(FORMAT_GENERATION) {
        return Ok(());
    }
    Err(WaymarkError::parse(
        path.to_string(),
        format!(
            "the document declares format generation {generation}, which this release cannot read (generation {FORMAT_GENERATION})"
        ),
    ))
}

/// Parse a document and additionally require its bytes to already be the
/// canonical form.
///
/// The round-trip invariant is stated in two halves: parsing this release's
/// output reproduces the item, and re-serializing it reproduces the bytes. This
/// helper asserts the second half.
pub fn parse_canonical(bytes: &[u8], path: &Utf8Path) -> Result<WorkItem, WaymarkError> {
    let item = parse_document(bytes, path)?;
    let rendered = crate::serialize::to_document(&item);
    if rendered.as_bytes() != bytes {
        return Err(WaymarkError::parse(
            path.to_string(),
            "document is not in canonical form",
        ));
    }
    Ok(item)
}

/// Parse the document at `path` and require its persisted ID to match the file
/// name stem.
pub fn parse_item_file(path: &Utf8Path) -> Result<WorkItem, WaymarkError> {
    let bytes = std::fs::read(path)
        .map_err(|source| WaymarkError::io(path.to_string(), "read", &source))?;
    let item = parse_document(&bytes, path)?;
    let stem = path.file_stem().unwrap_or_default();
    if item.id.as_str() != stem {
        return Err(WaymarkError::id_path_mismatch(&item.id, path, stem));
    }
    Ok(item)
}

/// The frontmatter as an ordered key/value list, plus the issues found while
/// reading it.
struct Frontmatter<'a> {
    path: &'a Utf8Path,
    entries: Vec<(String, Value)>,
    issues: Vec<ValidationIssue>,
}

impl<'a> Frontmatter<'a> {
    fn new(entries: Vec<(String, Value)>, path: &'a Utf8Path) -> Self {
        Self {
            path,
            entries,
            issues: Vec::new(),
        }
    }

    /// Consume the key/value list and validate every declared field.
    fn into_work_item(mut self, body: &str) -> Result<WorkItem, WaymarkError> {
        self.screen_keys();

        let schema = self.schema();
        let id = self.id("id");
        let title = self.required_string("title");
        let category = self.category();
        let status = self.status();
        let owner = self.required_string("owner");
        let creator = self.optional_string("creator");
        let created_at = self.timestamp("created_at");
        let updated_at = self.timestamp("updated_at");
        let closed_at = self.optional_timestamp("closed_at");
        let parent = self.optional_id("parent");
        let dependencies = self.id_list("dependencies");
        let soft_links = self.id_list("soft_links");
        let custom_fields = self.custom_fields();
        let archived_at = self.optional_timestamp("archived_at");
        let archive_reason = self.optional_string("archive_reason");

        if !self.issues.is_empty() {
            let target = format!("work item document {}", self.path);
            let location = self
                .issues
                .first()
                .map(|issue| Location::field(issue.path.clone()))
                .unwrap_or_else(|| Location::file(self.path.to_string()));
            return Err(WaymarkError::validation(target, self.issues).with_location(location));
        }

        Ok(WorkItem {
            id: id.expect("validation reported a missing id"),
            title: title.expect("validation reported a missing title"),
            category: category.expect("validation reported a missing category"),
            status: status.expect("validation reported a missing status"),
            owner: owner.expect("validation reported a missing owner"),
            creator,
            created_at: created_at.expect("validation reported a missing created_at"),
            updated_at: updated_at.expect("validation reported a missing updated_at"),
            closed_at,
            parent,
            dependencies,
            soft_links,
            custom_fields,
            archived_at,
            archive_reason,
            schema: schema.expect("validation reported a missing schema"),
            body: canonical_body(body),
        })
    }

    /// Split off the keys this release does not know, and duplicates.
    ///
    /// Both are reported before the values are looked at: an unknown key with a
    /// valid shape is still an error, and the message names the key so a human
    /// can fix the file.
    fn screen_keys(&mut self) {
        let mut seen: Vec<&str> = Vec::new();
        let mut keep: Vec<(String, Value)> = Vec::with_capacity(self.entries.len());
        for (key, value) in std::mem::take(&mut self.entries) {
            let Some(expected) = FRONTMATTER_KEYS.iter().find(|known| **known == key) else {
                self.issues.push(ValidationIssue::new(
                    key.clone(),
                    format!("unknown frontmatter key {key:?}"),
                ));
                continue;
            };
            if seen.contains(expected) {
                self.issues.push(ValidationIssue::new(
                    key.clone(),
                    format!("duplicate frontmatter key {key:?}"),
                ));
                continue;
            }
            seen.push(expected);
            keep.push((key, value));
        }
        for required in REQUIRED_KEYS {
            if !seen.contains(required) {
                self.issues.push(ValidationIssue::new(
                    *required,
                    format!("the required key {required:?} is missing"),
                ));
            }
        }
        self.entries = keep;
    }

    /// Take `key` out of the list.
    fn take(&mut self, key: &str) -> Option<Value> {
        let index = self.entries.iter().position(|(name, _)| name == key)?;
        Some(self.entries.remove(index).1)
    }

    fn issue(&mut self, key: &str, reason: impl Into<String>) {
        self.issues.push(ValidationIssue::new(key, reason));
    }

    /// A required string field.
    fn required_string(&mut self, key: &str) -> Option<String> {
        match self.take(key) {
            Some(Value::String(text)) => Some(text),
            None => None, // absence is already reported by `screen_keys`
            Some(_) => {
                self.issue(key, "must be a string");
                None
            }
        }
    }

    /// An optional string field; `null` is invalid, not "absent".
    fn optional_string(&mut self, key: &str) -> Option<String> {
        match self.take(key) {
            None => None,
            Some(Value::String(text)) => Some(text),
            Some(Value::Null) => {
                self.issue(key, "must not be null; omit the key instead");
                None
            }
            Some(_) => {
                self.issue(key, "must be a string");
                None
            }
        }
    }

    fn schema(&mut self) -> Option<u32> {
        let raw = self.take("schema")?;
        let Value::Number(number) = &raw else {
            self.issue("schema", "must be an integer format generation");
            return None;
        };
        let Some(generation) = number.as_u64() else {
            self.issue("schema", "must be a non-negative integer");
            return None;
        };
        let Ok(generation) = u32::try_from(generation) else {
            self.issue("schema", "must fit in 32 bits");
            return None;
        };
        // The generation itself was already checked by
        // `reject_unreadable_generation`, so it matches here.
        Some(generation)
    }

    fn id(&mut self, key: &str) -> Option<WorkItemId> {
        let raw = self.required_string(key)?;
        match WorkItemId::parse(&raw) {
            Ok(id) => Some(id),
            Err(source) => {
                self.issue(key, source.to_string());
                None
            }
        }
    }

    fn optional_id(&mut self, key: &str) -> Option<WorkItemId> {
        let raw = self.optional_string(key)?;
        match WorkItemId::parse(&raw) {
            Ok(id) => Some(id),
            Err(source) => {
                self.issue(key, source.to_string());
                None
            }
        }
    }

    fn category(&mut self) -> Option<CategoryKey> {
        let raw = self.required_string("category")?;
        match CategoryKey::parse(&raw) {
            Ok(category) => Some(category),
            Err(source) => {
                self.issue("category", source.to_string());
                None
            }
        }
    }

    fn status(&mut self) -> Option<Status> {
        let raw = self.required_string("status")?;
        match Status::parse(&raw) {
            Some(status) => Some(status),
            None => {
                self.issue(
                    "status",
                    format!("{raw:?} is not one of open, in_progress, closed"),
                );
                None
            }
        }
    }

    fn timestamp(&mut self, key: &str) -> Option<Timestamp> {
        let raw = self.required_string(key)?;
        match parse_utc(&raw) {
            Ok(value) => Some(Timestamp::new(value)),
            Err(source) => {
                self.issue(key, source.to_string());
                None
            }
        }
    }

    fn optional_timestamp(&mut self, key: &str) -> Option<Timestamp> {
        let raw = self.optional_string(key)?;
        match parse_utc(&raw) {
            Ok(value) => Some(Timestamp::new(value)),
            Err(source) => {
                self.issue(key, source.to_string());
                None
            }
        }
    }

    fn id_list(&mut self, key: &str) -> Vec<WorkItemId> {
        let Some(raw) = self.take(key) else {
            return Vec::new();
        };
        let items = match raw {
            // `[]` is the hand-edited spelling of "empty"; absent and empty mean
            // the same thing.
            Value::Sequence(items) => items,
            Value::Null => {
                self.issue(key, "must not be null; omit the key instead");
                return Vec::new();
            }
            _ => {
                self.issue(key, "must be a list of stable IDs");
                return Vec::new();
            }
        };
        let mut ids = Vec::with_capacity(items.len());
        for item in items {
            let Value::String(text) = item else {
                self.issue(key, "must be a list of stable IDs");
                continue;
            };
            match WorkItemId::parse(&text) {
                Ok(id) => ids.push(id),
                Err(source) => self.issue(key, source.to_string()),
            }
        }
        // The canonical writer emits relationships sorted and de-duplicated.
        // Normalizing on read keeps `parse(to_document(item)) == item` true for
        // every item, including one a hand edit left in a different order; order
        // is not information the store keeps, so this loses nothing.
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn custom_fields(&mut self) -> CustomFields {
        let Some(raw) = self.take("custom_fields") else {
            return CustomFields::new();
        };
        let entries = match raw {
            Value::Mapping(mapping) => mapping.into_iter().collect::<Vec<(Value, Value)>>(),
            Value::Null => {
                self.issue("custom_fields", "must not be null; omit the key instead");
                return CustomFields::new();
            }
            _ => {
                self.issue("custom_fields", "must be a mapping of keys to values");
                return CustomFields::new();
            }
        };
        let mut fields = CustomFields::new();
        for (key, value) in entries {
            let Value::String(key) = key else {
                self.issue("custom_fields", "keys must be strings");
                continue;
            };
            let path = format!("custom_fields.{key}");
            if FieldKey::parse(&key).is_err() {
                self.issues.push(ValidationIssue::new(
                    path.clone(),
                    "not a valid custom-field key (lowercase ASCII snake_case)",
                ));
                continue;
            }
            if crate::model::RESERVED_FIELD_NAMES.contains(&key.as_str()) {
                self.issues.push(ValidationIssue::new(
                    path.clone(),
                    "a custom field may not shadow a reserved core or computed name",
                ));
                continue;
            }
            if let Some(field) = self.field_value(&path, value) {
                fields.insert(key, field);
            }
        }
        fields
    }

    /// Convert one custom-field value, reporting rather than repairing a shape
    /// the field types cannot express.
    fn field_value(&mut self, path: &str, value: Value) -> Option<FieldValue> {
        match value {
            Value::String(text) => Some(FieldValue::String(text)),
            Value::Bool(flag) => Some(FieldValue::Boolean(flag)),
            Value::Number(number) => match number.as_i64() {
                Some(number) => Some(FieldValue::Integer(number)),
                None => {
                    self.issues.push(ValidationIssue::new(
                        path,
                        "a custom-field integer must be a whole number",
                    ));
                    None
                }
            },
            Value::Sequence(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    match item {
                        Value::String(text) => values.push(FieldValue::String(text)),
                        Value::Bool(flag) => values.push(FieldValue::Boolean(flag)),
                        Value::Number(number) => match number.as_i64() {
                            Some(number) => values.push(FieldValue::Integer(number)),
                            None => {
                                self.issues.push(ValidationIssue::new(
                                    path,
                                    "a custom-field integer must be a whole number",
                                ));
                                return None;
                            }
                        },
                        Value::Sequence(_) | Value::Mapping(_) => {
                            self.issues.push(ValidationIssue::new(
                                path,
                                "a custom-field list must be a flat list of scalars",
                            ));
                            return None;
                        }
                        Value::Null => {
                            self.issues.push(ValidationIssue::new(
                                path,
                                "a custom-field list must not contain null",
                            ));
                            return None;
                        }
                        Value::Tagged(_) => {
                            self.issues.push(ValidationIssue::new(
                                path,
                                "a custom-field list must be a flat list of scalars",
                            ));
                            return None;
                        }
                    }
                }
                Some(FieldValue::List(values))
            }
            Value::Null => {
                self.issues.push(ValidationIssue::new(
                    path,
                    "null is not a valid value; omit the key",
                ));
                None
            }
            Value::Mapping(_) => {
                self.issues.push(ValidationIssue::new(
                    path,
                    "a custom-field value may not be a nested object",
                ));
                None
            }
            Value::Tagged(_) => {
                self.issues.push(ValidationIssue::new(
                    path,
                    "a custom-field value may not be a tagged YAML value",
                ));
                None
            }
        }
    }
}

/// Turn a `serde_norway` failure into a locatable `PARSE_ERROR`, using the
/// library's 1-based line/column when it reports one.
fn yaml_error(path: &Utf8Path, source: &serde_norway::Error) -> WaymarkError {
    let location = match source.location() {
        Some(location) => Location::File {
            path: path.to_string(),
            line: Some(location.line() as u32),
            column: Some(location.column() as u32),
        },
        None => Location::file(path.to_string()),
    };
    WaymarkError::parse(path.to_string(), source.to_string()).with_location(location)
}

/// Split a document into (frontmatter bytes, body bytes) by locating the opening
/// and closing `---` fence lines. The frontmatter slice excludes the fences; the
/// body slice is everything after the closing fence's line terminator.
fn split_frontmatter<'a>(
    bytes: &'a [u8],
    path: &Utf8Path,
) -> Result<(&'a [u8], &'a [u8]), WaymarkError> {
    let open_len = if bytes.starts_with(b"---\n") {
        4
    } else if bytes.starts_with(b"---\r\n") {
        5
    } else {
        return Err(WaymarkError::parse(
            path.to_string(),
            "a document must begin with a `---` frontmatter fence",
        ));
    };

    let mut search_from = open_len;
    while search_from + 3 <= bytes.len() {
        let Some(relative) = find_subslice(&bytes[search_from..], b"---") else {
            break;
        };
        let fence_start = search_from + relative;
        let after_fence = fence_start + 3;
        let at_line_start = fence_start > 0 && bytes[fence_start - 1] == b'\n';
        let body_start = if after_fence == bytes.len() {
            Some(after_fence)
        } else if bytes[after_fence] == b'\n' {
            Some(after_fence + 1)
        } else if bytes[after_fence] == b'\r' && bytes.get(after_fence + 1) == Some(&b'\n') {
            Some(after_fence + 2)
        } else {
            None
        };
        if let (true, Some(body_start)) = (at_line_start, body_start) {
            let frontmatter_end = fence_start - 1;
            let frontmatter = &bytes[open_len..frontmatter_end.max(open_len)];
            return Ok((frontmatter, &bytes[body_start..]));
        }
        search_from = after_fence;
    }
    Err(WaymarkError::parse(
        path.to_string(),
        "the frontmatter has no closing `---` fence",
    ))
}

/// A byte-substring search that does not need an extra dependency.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Reject YAML anchors (`&name`) and aliases (`*name`) at node positions before
/// the YAML parser can expand them.
///
/// The scan is quote-aware and block-scalar-aware on purpose: the canonical
/// writer emits values such as `title: 'Fix: *very* broken'`, and a naive scan
/// would reject a file Waymark had just written — a write that succeeds and a
/// read that then fails. `&`/`*` mid-scalar (`R&D`, `a - *b*`) are plain text.
pub fn contains_yaml_anchor_or_alias(frontmatter: &[u8]) -> bool {
    let mut block_scalar_indent: Option<usize> = None;
    let mut flow_depth = 0usize;

    for line in frontmatter.split(|&byte| byte == b'\n') {
        let indent = line
            .iter()
            .take_while(|&&byte| byte == b' ' || byte == b'\t')
            .count();
        let blank = indent == line.len();
        if let Some(opener_indent) = block_scalar_indent {
            if blank || indent > opener_indent {
                continue;
            }
            block_scalar_indent = None;
        }
        if scan_line_for_anchor(line, &mut flow_depth) {
            return true;
        }
        if opens_block_scalar(line) {
            block_scalar_indent = Some(indent);
        }
    }
    false
}

/// Whether a line's value opens a YAML block scalar (`key: |`, `key: >2-`).
fn opens_block_scalar(line: &[u8]) -> bool {
    let mut end = line.len();
    while end > 0 && matches!(line[end - 1], b' ' | b'\t' | b'\r') {
        end -= 1;
    }
    while end > 0 && matches!(line[end - 1], b'+' | b'-' | b'0'..=b'9') {
        end -= 1;
    }
    end > 0
        && matches!(line[end - 1], b'|' | b'>')
        && (end == 1 || matches!(line[end - 2], b' ' | b'\t' | b':'))
}

fn scan_line_for_anchor(line: &[u8], flow_depth: &mut usize) -> bool {
    let mut quote: Option<u8> = None;
    let mut index = 0;
    while index < line.len() {
        let byte = line[index];
        if let Some(open) = quote {
            if byte == open {
                if open == b'\'' && line.get(index + 1) == Some(&b'\'') {
                    index += 1;
                } else {
                    quote = None;
                }
            } else if open == b'"' && byte == b'\\' {
                index += 1;
            }
            index += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' if at_node_start(line, index, *flow_depth) => quote = Some(byte),
            b'[' | b'{' => *flow_depth += 1,
            b']' | b'}' => *flow_depth = flow_depth.saturating_sub(1),
            b'&' | b'*' => {
                let named = line
                    .get(index + 1)
                    .is_some_and(|&next| next.is_ascii_alphanumeric() || next == b'_');
                if named && at_node_start(line, index, *flow_depth) {
                    return true;
                }
            }
            _ => {}
        }
        index += 1;
    }
    false
}

/// Whether `index` is a YAML node start: after `key: `, after a flow `[`/`{`,
/// after a `,` in flow context, or after a block-sequence dash chain reaching
/// the start of the line.
fn at_node_start(bytes: &[u8], index: usize, flow_depth: usize) -> bool {
    let mut back = index;
    let mut crossed_dash = false;
    loop {
        let start = back;
        while back > 0 && matches!(bytes[back - 1], b' ' | b'\t') {
            back -= 1;
        }
        let had_space = back < start;
        if back == 0 {
            return crossed_dash;
        }
        return match bytes[back - 1] {
            b'[' | b'{' => true,
            b',' => flow_depth > 0,
            b':' => had_space,
            b'-' if had_space => {
                crossed_dash = true;
                back -= 1;
                continue;
            }
            _ => false,
        };
    }
}
