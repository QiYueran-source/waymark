//! The collection query model: filters, ordering, pagination, and projection.
//!
//! `list`, `ready`, `blocked`, `query`, and `search` answer different questions
//! but share this one vocabulary. There is deliberately no second filter struct,
//! sort enum, or window type anywhere in the crate: an adapter that wants a new
//! collection flag adds it here.
//!
//! # Filters
//!
//! `Filters` is a conjunction. Every member is optional, and an empty `Filters`
//! matches every item its population allows. `null` is never a filter value:
//! absence is expressed by omitting the member, presence by [`Filters::exists`]
//! or [`Filters::has_dependencies`].
//!
//! Custom-field predicates are typed against the project's declarations
//! ([`Filters::validate`]); the matcher itself is type-blind so a malformed
//! predicate can never silently match.
//!
//! # Ordering
//!
//! [`SortField::Rank`] is the default: status group, then dependency
//! topological order, then stable ID. Topological order is computed once over
//! the whole active project so a page's rank does not depend on which other
//! filters are set. Items that a dependency cycle keeps out of the topological
//! order are appended in stable-ID order, which keeps the result total and
//! deterministic. The final tie-break is always the stable ID, ascending, even
//! when the primary order is descending.
//!
//! # Pagination
//!
//! A [`Window`] slices one in-memory snapshot after sorting. `limit` defaults to
//! [`DEFAULT_LIMIT`] (100) for the CLI, [`MCP_DEFAULT_LIMIT`] (50) is offered for
//! the MCP adapter, and `0` means unlimited. The listing reports `total` as the
//! pre-window count, so a caller can tell a short page from a complete answer.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use crate::config::{Config, FieldType};
use crate::envelope::{Finding, Meta};
use crate::error::{ValidationIssue, WaymarkError};
use crate::id::WorkItemId;
use crate::location::Location;
pub use crate::model::FieldKey;
use crate::model::{CategoryKey, FieldValue, Status, WorkItem};
use crate::readiness::{Graph, Readiness};

/// The CLI's default page size.
pub const DEFAULT_LIMIT: usize = 100;

/// The MCP adapter's default page size (reserved for T10).
pub const MCP_DEFAULT_LIMIT: usize = 50;

/// Which archived slice a collection read covers.
///
/// The default is [`Self::Active`]: a collection query never mixes archived and
/// active items unless the caller asks for it explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArchiveRange {
    /// Active items only.
    #[default]
    Active,
    /// Archived items only.
    Archived,
    /// Both.
    All,
}

impl ArchiveRange {
    /// The literal used on the wire and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::All => "all",
        }
    }

    /// Parse the wire literal.
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    /// Whether an item with this archive state belongs to the range.
    pub fn includes(self, archived: bool) -> bool {
        match self {
            Self::Active => !archived,
            Self::Archived => archived,
            Self::All => true,
        }
    }
}

/// The readiness predicate a collection read can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessFilter {
    /// Not closed, every dependency closed.
    Ready,
    /// Not closed, at least one dependency unfinished or missing.
    Blocked,
}

/// One custom-field predicate.
///
/// The operand is optional only for the two presence operations; every other
/// operation requires one, and [`Filters::validate`] enforces that against the
/// declared field type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPredicate {
    /// The declared custom field.
    pub key: FieldKey,
    /// What to test.
    pub op: FieldOp,
    /// The operand, absent for presence tests.
    pub value: Option<FieldValue>,
}

impl FieldPredicate {
    /// A presence test.
    pub fn present(key: FieldKey) -> Self {
        Self {
            key,
            op: FieldOp::Exists,
            value: None,
        }
    }

    /// A predicate with an operand.
    pub fn new(key: FieldKey, op: FieldOp, value: FieldValue) -> Self {
        Self {
            key,
            op,
            value: Some(value),
        }
    }

    /// Parse the structured JSON form: `{key, op, value?}`.
    pub fn from_json(value: &Value) -> Result<Self, WaymarkError> {
        let object = value.as_object().ok_or_else(|| {
            invalid(
                "field",
                "a field predicate must be a JSON object with `key`, `op`, and `value`",
            )
        })?;
        let key = object
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("field.key", "a field predicate needs a string `key`"))?;
        let key = FieldKey::parse(key).map_err(|error| invalid("field.key", error.to_string()))?;
        let op_literal = object
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("field.op", "a field predicate needs a string `op`"))?;
        let op = FieldOp::parse(op_literal).ok_or_else(|| {
            invalid(
                "field.op",
                format!("{op_literal:?} is not a field operation"),
            )
        })?;
        let raw = object.get("value");
        let value = match (op, raw) {
            (FieldOp::Exists | FieldOp::Missing, None) => None,
            (FieldOp::Exists | FieldOp::Missing, Some(_)) => {
                return Err(invalid(
                    "field.value",
                    "a presence test does not take a value",
                ))
            }
            (_, None) => {
                return Err(invalid(
                    "field.value",
                    format!("{:?} needs a value", op.as_str()),
                ))
            }
            (_, Some(raw)) => Some(json_to_field_value(raw)?),
        };
        for known in object.keys() {
            if !matches!(known.as_str(), "key" | "op" | "value") {
                return Err(invalid(
                    format!("field.{known}"),
                    "a field predicate accepts only `key`, `op`, and `value`",
                ));
            }
        }
        Ok(Self { key, op, value })
    }
}

/// The finite custom-field operation vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldOp {
    /// The field has a value.
    Exists,
    /// The field has no value.
    Missing,
    /// Scalar equality.
    Eq,
    /// Scalar inequality.
    Ne,
    /// Lexicographic or numeric less-than.
    Lt,
    /// Less-than-or-equal.
    Le,
    /// Greater-than.
    Gt,
    /// Greater-than-or-equal.
    Ge,
    /// Case-sensitive string prefix.
    Prefix,
    /// Case-sensitive string substring.
    Contains,
    /// Whole-list equality.
    ListEq,
    /// Whole-list inequality.
    ListNe,
    /// The list contains one element equal to the operand.
    ListContains,
}

impl FieldOp {
    /// The wire literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exists => "exists",
            Self::Missing => "missing",
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
            Self::Prefix => "prefix",
            Self::Contains => "contains",
            Self::ListEq => "list_eq",
            Self::ListNe => "list_ne",
            Self::ListContains => "list_contains",
        }
    }

    /// Parse the wire literal.
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "exists" => Some(Self::Exists),
            "missing" => Some(Self::Missing),
            "eq" => Some(Self::Eq),
            "ne" => Some(Self::Ne),
            "lt" => Some(Self::Lt),
            "le" => Some(Self::Le),
            "gt" => Some(Self::Gt),
            "ge" => Some(Self::Ge),
            "prefix" => Some(Self::Prefix),
            "contains" => Some(Self::Contains),
            "list_eq" => Some(Self::ListEq),
            "list_ne" => Some(Self::ListNe),
            "list_contains" => Some(Self::ListContains),
            _ => None,
        }
    }

    /// Whether the operation takes an operand.
    pub fn takes_value(self) -> bool {
        !matches!(self, Self::Exists | Self::Missing)
    }
}

/// The shared filter vocabulary.
///
/// `with_status` and `with_category` stay as single-value builders so existing
/// callers keep compiling; both add to a set, because a collection query may ask
/// for several statuses or categories at once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    /// Only items in one of these statuses; empty means any.
    pub statuses: BTreeSet<Status>,
    /// Only items in one of these categories; empty means any.
    pub categories: BTreeSet<CategoryKey>,
    /// Only items with this exact owner.
    pub owner: Option<String>,
    /// Only items with this exact creator.
    pub creator: Option<String>,
    /// Only children of this item.
    pub parent: Option<WorkItemId>,
    /// `Some(true)` keeps items with a parent, `Some(false)` keeps roots.
    pub parent_present: Option<bool>,
    /// `Some(true)` keeps items that list at least one dependency.
    pub has_dependencies: Option<bool>,
    /// Only ready or only blocked items.
    pub readiness: Option<ReadinessFilter>,
    /// Custom-field predicates, all of which must hold.
    pub fields: Vec<FieldPredicate>,
    /// Cheap text: a case-insensitive substring of the ID or the title only.
    pub text: Option<String>,
    /// Keep only items with no children.
    pub leaves_only: bool,
    /// Which archive slice to read.
    pub archive: ArchiveRange,
}

impl Filters {
    /// No filters, active items.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a status.
    pub fn with_status(mut self, status: Status) -> Self {
        self.statuses.insert(status);
        self
    }

    /// Add a category.
    pub fn with_category(mut self, category: CategoryKey) -> Self {
        self.categories.insert(category);
        self
    }

    /// Restrict to one owner.
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// Restrict to one creator.
    pub fn with_creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    /// Require a value for `key`.
    pub fn with_field(mut self, predicate: FieldPredicate) -> Self {
        self.fields.push(predicate);
        self
    }

    /// Add a readiness predicate.
    pub fn with_readiness(mut self, readiness: ReadinessFilter) -> Self {
        self.readiness = Some(readiness);
        self
    }

    /// Select the archive slice.
    pub fn with_archive(mut self, range: ArchiveRange) -> Self {
        self.archive = range;
        self
    }

    /// Whether `item` survives every filter.
    ///
    /// `readiness` is the derived readiness when the item is an active node and
    /// `None` for an archived one; `child_count` is the number of active
    /// children. Both are supplied by the caller because deriving them is a
    /// graph operation, not a filter.
    pub fn matches(
        &self,
        item: &WorkItem,
        readiness: Option<&Readiness>,
        child_count: usize,
    ) -> bool {
        if !self.archive.includes(item.is_archived()) {
            return false;
        }
        if !self.statuses.is_empty() && !self.statuses.contains(&item.status) {
            return false;
        }
        if !self.categories.is_empty() && !self.categories.contains(&item.category) {
            return false;
        }
        if let Some(owner) = &self.owner {
            if &item.owner != owner {
                return false;
            }
        }
        if let Some(creator) = &self.creator {
            if item.creator.as_deref() != Some(creator.as_str()) {
                return false;
            }
        }
        if let Some(parent) = &self.parent {
            if item.parent.as_ref() != Some(parent) {
                return false;
            }
        }
        if let Some(present) = self.parent_present {
            if item.parent.is_some() != present {
                return false;
            }
        }
        if let Some(present) = self.has_dependencies {
            if !item.dependencies.is_empty() != present {
                return false;
            }
        }
        if self.leaves_only && child_count != 0 {
            return false;
        }
        if let Some(wanted) = self.readiness {
            let Some(readiness) = readiness else {
                return false;
            };
            let matches = match wanted {
                ReadinessFilter::Ready => readiness.ready,
                ReadinessFilter::Blocked => readiness.blocked,
            };
            if !matches {
                return false;
            }
        }
        if let Some(text) = &self.text {
            let needle = text.to_lowercase();
            let id_hit = item.id.as_str().to_lowercase().contains(&needle);
            let title_hit = item.title.to_lowercase().contains(&needle);
            if !id_hit && !title_hit {
                return false;
            }
        }
        self.fields
            .iter()
            .all(|predicate| predicate_matches(item, predicate))
    }

    /// Validate the predicates and operand types against the declared fields.
    pub fn validate(&self, config: &Config) -> Result<(), WaymarkError> {
        let mut issues = Vec::new();
        for predicate in &self.fields {
            validate_predicate(config, predicate, &mut issues);
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(WaymarkError::validation("query", issues).with_location(Location::field("filter")))
        }
    }

    /// Parse the structured JSON filter form used by `waymark query`.
    pub fn from_json(value: &Value) -> Result<Self, WaymarkError> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("filter", "the structured filter must be a JSON object"))?;
        let mut filters = Self::new();
        for (key, entry) in object {
            match key.as_str() {
                "status" => {
                    for status in string_set(entry, "status")? {
                        let parsed = Status::parse(&status).ok_or_else(|| {
                            invalid(
                                "status",
                                format!("{status:?} is not one of open, in_progress, closed"),
                            )
                        })?;
                        filters.statuses.insert(parsed);
                    }
                }
                "category" => {
                    for category in string_set(entry, "category")? {
                        filters.categories.insert(
                            CategoryKey::parse(&category)
                                .map_err(|error| invalid("category", error.to_string()))?,
                        );
                    }
                }
                "owner" => filters.owner = Some(string_value(entry, "owner")?),
                "creator" => filters.creator = Some(string_value(entry, "creator")?),
                "parent" => {
                    let literal = string_value(entry, "parent")?;
                    filters.parent = Some(
                        WorkItemId::parse(&literal)
                            .map_err(|error| invalid("parent", error.to_string()))?,
                    );
                }
                "parent_present" => {
                    filters.parent_present = Some(bool_value(entry, "parent_present")?)
                }
                "dependencies" => {
                    filters.has_dependencies = Some(match entry {
                        Value::Bool(flag) => *flag,
                        other => match string_value(other, "dependencies")?.as_str() {
                            "any" | "has" => true,
                            "none" => false,
                            other => {
                                return Err(invalid(
                                    "dependencies",
                                    format!("{other:?} is not `any` or `none`"),
                                ))
                            }
                        },
                    });
                }
                "ready" => {
                    let ready = bool_value(entry, "ready")?;
                    filters.readiness = Some(if ready {
                        ReadinessFilter::Ready
                    } else {
                        ReadinessFilter::Blocked
                    });
                }
                "blocked" => {
                    let blocked = bool_value(entry, "blocked")?;
                    filters.readiness = Some(if blocked {
                        ReadinessFilter::Blocked
                    } else {
                        ReadinessFilter::Ready
                    });
                }
                "field" | "fields" => {
                    let entries = entry.as_array().ok_or_else(|| {
                        invalid("field", "`field` must be an array of predicates")
                    })?;
                    for predicate in entries {
                        filters.fields.push(FieldPredicate::from_json(predicate)?);
                    }
                }
                "text" => filters.text = Some(string_value(entry, "text")?),
                "leaves" => filters.leaves_only = bool_value(entry, "leaves")?,
                "archive" => {
                    let literal = string_value(entry, "archive")?;
                    filters.archive = ArchiveRange::parse(&literal).ok_or_else(|| {
                        invalid(
                            "archive",
                            format!("{literal:?} is not `active`, `archived`, or `all`"),
                        )
                    })?;
                }
                other => {
                    return Err(invalid(
                        format!("filter.{other}"),
                        "unknown structured filter key",
                    ))
                }
            }
        }
        Ok(filters)
    }
}

fn invalid(path: impl Into<String>, reason: impl Into<String>) -> WaymarkError {
    WaymarkError::validation("filter", vec![ValidationIssue::new(path, reason)]).at_field("filter")
}

fn string_value(value: &Value, path: &str) -> Result<String, WaymarkError> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid(path, "expected a string; null is not a filter value"))
}

fn bool_value(value: &Value, path: &str) -> Result<bool, WaymarkError> {
    value
        .as_bool()
        .ok_or_else(|| invalid(path, "expected true or false; null is not a filter value"))
}

fn string_set(value: &Value, path: &str) -> Result<Vec<String>, WaymarkError> {
    match value {
        Value::String(text) => Ok(vec![text.clone()]),
        Value::Array(items) => items.iter().map(|item| string_value(item, path)).collect(),
        _ => Err(invalid(path, "expected a string or an array of strings")),
    }
}

/// Convert a JSON operand to a typed custom value, rejecting floats and null.
fn json_to_field_value(value: &Value) -> Result<FieldValue, WaymarkError> {
    match value {
        Value::Null => Err(invalid(
            "field.value",
            "null is never a filter value; use `missing`",
        )),
        Value::Bool(flag) => Ok(FieldValue::Boolean(*flag)),
        Value::Number(number) => number
            .as_i64()
            .map(FieldValue::Integer)
            .ok_or_else(|| invalid("field.value", "custom integers are whole 64-bit numbers")),
        Value::String(text) => Ok(FieldValue::String(text.clone())),
        Value::Array(items) => items
            .iter()
            .map(json_to_field_value)
            .collect::<Result<Vec<_>, _>>()
            .map(FieldValue::List),
        Value::Object(_) => Err(invalid(
            "field.value",
            "custom values are scalars or homogeneous lists, never objects",
        )),
    }
}

fn validate_predicate(
    config: &Config,
    predicate: &FieldPredicate,
    issues: &mut Vec<ValidationIssue>,
) {
    let path = format!("field.{}", predicate.key);
    let Some(declaration) = config.fields.get(&predicate.key) else {
        issues.push(ValidationIssue::new(
            path,
            "no declared custom field has this key",
        ));
        return;
    };
    let ty = &declaration.ty;
    let mismatched = || {
        ValidationIssue::new(
            path.clone(),
            format!(
                "{:?} does not match the declared {} field",
                predicate.value,
                ty.as_str()
            ),
        )
    };
    match predicate.op {
        FieldOp::Exists | FieldOp::Missing => {
            if predicate.value.is_some() {
                issues.push(ValidationIssue::new(
                    path,
                    "a presence test does not take a value",
                ));
            }
        }
        FieldOp::Eq | FieldOp::Ne => match ty {
            FieldType::List(_) => issues.push(ValidationIssue::new(
                path,
                "use `list_eq` or `list_ne` for a list field",
            )),
            _ => match &predicate.value {
                Some(value) if ty.accepts(value) => {}
                Some(_) => issues.push(mismatched()),
                None => issues.push(ValidationIssue::new(path, "this operation needs a value")),
            },
        },
        FieldOp::Lt | FieldOp::Le | FieldOp::Gt | FieldOp::Ge => match ty {
            FieldType::String => match &predicate.value {
                Some(FieldValue::String(_)) => {}
                _ => issues.push(ValidationIssue::new(
                    path,
                    "only string and integer fields can be ordered",
                )),
            },
            FieldType::Integer => match &predicate.value {
                Some(FieldValue::Integer(_)) => {}
                _ => issues.push(ValidationIssue::new(
                    path,
                    "only string and integer fields can be ordered",
                )),
            },
            _ => issues.push(ValidationIssue::new(
                path,
                "only string and integer fields can be ordered",
            )),
        },
        FieldOp::Prefix | FieldOp::Contains => match ty {
            FieldType::String => match &predicate.value {
                Some(FieldValue::String(_)) => {}
                _ => issues.push(ValidationIssue::new(
                    path,
                    "string matching needs a string operand",
                )),
            },
            _ => issues.push(ValidationIssue::new(
                path,
                "prefix and substring matching apply to string fields only",
            )),
        },
        FieldOp::ListEq | FieldOp::ListNe => match ty {
            FieldType::List(_) => match &predicate.value {
                Some(value) if ty.accepts(value) => {}
                _ => issues.push(ValidationIssue::new(
                    path,
                    "whole-list equality needs a list of the declared element type",
                )),
            },
            _ => issues.push(ValidationIssue::new(
                path,
                "whole-list equality applies to list fields only",
            )),
        },
        FieldOp::ListContains => match ty {
            FieldType::List(element) => match &predicate.value {
                Some(value) if element.accepts(value) => {}
                _ => issues.push(ValidationIssue::new(
                    path,
                    "list membership needs one element of the declared element type",
                )),
            },
            _ => issues.push(ValidationIssue::new(
                path,
                "list membership applies to list fields only",
            )),
        },
    }
}

fn predicate_matches(item: &WorkItem, predicate: &FieldPredicate) -> bool {
    let value = item.custom_fields.get(predicate.key.as_str());
    match predicate.op {
        FieldOp::Exists => value.is_some(),
        FieldOp::Missing => value.is_none(),
        _ => {
            let (Some(value), Some(operand)) = (value, &predicate.value) else {
                return false;
            };
            match predicate.op {
                FieldOp::Eq => value == operand,
                FieldOp::Ne => value != operand,
                FieldOp::Lt => compare_scalars(value, operand) == Some(Ordering::Less),
                FieldOp::Le => matches!(
                    compare_scalars(value, operand),
                    Some(Ordering::Less | Ordering::Equal)
                ),
                FieldOp::Gt => compare_scalars(value, operand) == Some(Ordering::Greater),
                FieldOp::Ge => matches!(
                    compare_scalars(value, operand),
                    Some(Ordering::Greater | Ordering::Equal)
                ),
                FieldOp::Prefix => match (value, operand) {
                    (FieldValue::String(value), FieldValue::String(prefix)) => {
                        value.starts_with(prefix)
                    }
                    _ => false,
                },
                FieldOp::Contains => match (value, operand) {
                    (FieldValue::String(value), FieldValue::String(needle)) => {
                        value.contains(needle)
                    }
                    _ => false,
                },
                FieldOp::ListEq => value == operand,
                FieldOp::ListNe => value != operand,
                FieldOp::ListContains => match value {
                    FieldValue::List(values) => values.contains(operand),
                    _ => false,
                },
                FieldOp::Exists | FieldOp::Missing => unreachable!("handled above"),
            }
        }
    }
}

/// Compare two scalar custom values, or `None` when they are not comparable.
fn compare_scalars(left: &FieldValue, right: &FieldValue) -> Option<Ordering> {
    match (left, right) {
        (FieldValue::String(left), FieldValue::String(right)) => Some(left.cmp(right)),
        (FieldValue::Integer(left), FieldValue::Integer(right)) => Some(left.cmp(right)),
        _ => None,
    }
}

/// The finite sort vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortField {
    /// Status group, dependency topological order, stable ID.
    Rank,
    /// Creation instant.
    CreatedAt,
    /// Last-change instant.
    UpdatedAt,
    /// Title, byte order.
    Title,
    /// Lifecycle state, in group order.
    Status,
    /// One declared string or integer custom field.
    Custom(FieldKey),
}

impl SortField {
    /// The wire and CLI literal (`field:<key>` for a custom sort).
    pub fn as_str(&self) -> String {
        match self {
            Self::Rank => "rank".to_owned(),
            Self::CreatedAt => "created_at".to_owned(),
            Self::UpdatedAt => "updated_at".to_owned(),
            Self::Title => "title".to_owned(),
            Self::Status => "status".to_owned(),
            Self::Custom(key) => format!("field:{key}"),
        }
    }

    /// Parse the wire and CLI literal.
    pub fn parse(input: &str) -> Result<Self, WaymarkError> {
        let trimmed = input.trim();
        match trimmed {
            "rank" => Ok(Self::Rank),
            "created_at" => Ok(Self::CreatedAt),
            "updated_at" => Ok(Self::UpdatedAt),
            "title" => Ok(Self::Title),
            "status" => Ok(Self::Status),
            other => {
                let Some(key) = other.strip_prefix("field:") else {
                    return Err(invalid(
                        "sort",
                        format!(
                            "{other:?} is not a sort field; expected rank, created_at, updated_at, title, status, or field:<key>"
                        ),
                    ));
                };
                let key =
                    FieldKey::parse(key).map_err(|error| invalid("sort", error.to_string()))?;
                Ok(Self::Custom(key))
            }
        }
    }
}

/// One ordering: a field plus a direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sort {
    /// The primary order.
    pub field: SortField,
    /// Reverse the primary comparison; the stable-ID tie-break stays ascending.
    pub descending: bool,
}

impl Sort {
    /// The default `rank` ordering.
    pub fn rank() -> Self {
        Self {
            field: SortField::Rank,
            descending: false,
        }
    }

    /// Ascending order by `field`.
    pub fn of(field: SortField) -> Self {
        Self {
            field,
            descending: false,
        }
    }

    /// Reverse the primary comparison.
    pub fn descending(mut self) -> Self {
        self.descending = true;
        self
    }

    /// Validate a custom sort against the declared fields.
    pub fn validate(&self, config: &Config) -> Result<(), WaymarkError> {
        let SortField::Custom(key) = &self.field else {
            return Ok(());
        };
        let Some(declaration) = config.fields.get(key) else {
            return Err(invalid(
                "sort",
                format!("no declared custom field has the key {key:?}"),
            ));
        };
        match declaration.ty {
            FieldType::String | FieldType::Integer => Ok(()),
            _ => Err(invalid(
                "sort",
                format!(
                    "{} fields are not sortable; only strings and integers are",
                    declaration.ty.as_str()
                ),
            )),
        }
    }
}

impl Default for Sort {
    fn default() -> Self {
        Self::rank()
    }
}

/// The snapshot-local page window.
///
/// Applied after sorting. `limit == 0` means unlimited, and `offset` skips whole
/// results; the two never interact with a second read, so a page cannot skip or
/// repeat an item within one snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    /// Maximum rows to return; `0` is unlimited.
    pub limit: usize,
    /// Rows to skip.
    pub offset: usize,
}

impl Window {
    /// The CLI default: 100 rows, no offset.
    pub fn new() -> Self {
        Self {
            limit: DEFAULT_LIMIT,
            offset: 0,
        }
    }

    /// A window with an explicit limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// A window with an explicit offset.
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// The effective limit, with `0` meaning "everything left".
    pub fn effective_limit(&self, total: usize) -> usize {
        if self.limit == 0 {
            total.saturating_sub(self.offset)
        } else {
            self.limit
        }
    }

    /// Slice `rows`, returning the window contents.
    pub fn apply<T>(&self, rows: Vec<T>) -> Vec<T> {
        let start = self.offset.min(rows.len());
        let taken = rows
            .into_iter()
            .skip(start)
            .take(self.effective_limit(usize::MAX));
        taken.collect()
    }
}

impl Default for Window {
    fn default() -> Self {
        Self::new()
    }
}

/// One row of a collection result: the item plus its derived columns.
#[derive(Debug, Clone)]
pub struct ListedItem {
    /// The document.
    pub item: WorkItem,
    /// Its derived readiness. An archived item is neither ready nor blocked.
    pub readiness: Readiness,
    /// Its active children, sorted by stable ID.
    pub children: Vec<WorkItemId>,
    /// The active items that depend on it, sorted by stable ID.
    pub dependents: Vec<WorkItemId>,
}

/// The result of a list-family read.
#[derive(Debug, Clone)]
pub struct Listing {
    /// The matching items in the requested order, sliced to the window.
    pub items: Vec<ListedItem>,
    /// The number of matches before the window was applied.
    pub total: usize,
    /// How many matches were skipped.
    pub offset: usize,
    /// The effective limit (`0` echoed as unlimited).
    pub limit: usize,
    /// The `_meta` block: complete, fresh, with the graph's diagnostics.
    pub meta: Meta,
}

impl Listing {
    /// The number of returned items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing matched.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The items only, for a caller that does not need the derived columns.
    pub fn into_items(self) -> Vec<WorkItem> {
        self.items.into_iter().map(|row| row.item).collect()
    }
}

/// Assemble a listing from the full, already sorted match set.
pub fn listing_from_sorted(rows: Vec<ListedItem>, window: &Window, meta: Meta) -> Listing {
    let total = rows.len();
    let offset = window.offset;
    let limit = window.limit;
    let items = window.apply(rows);
    Listing {
        items,
        total,
        offset,
        limit,
        meta,
    }
}

/// The whole-project scan a collection read answers from.
///
/// Reading is a complete snapshot: every document is parsed once, archived items
/// are kept aside, and the graph is built from the active ones. See
/// [`crate::readiness::Graph::from_project`] for the deliberate decision to fail
/// the whole read on an unparseable sibling rather than return a partial page.
#[derive(Debug, Clone)]
pub struct Snapshot {
    graph: Graph,
    archived: Vec<WorkItem>,
    ranks: BTreeMap<WorkItemId, usize>,
}

impl Snapshot {
    /// Scan the project.
    pub fn load(project: &crate::store::Project) -> Result<Self, WaymarkError> {
        let inventory = project.inventory()?;
        let mut all = Vec::with_capacity(inventory.entries.len());
        for entry in &inventory.entries {
            all.push(crate::parse::parse_item_file(&entry.path)?);
        }
        let graph = Graph::from_items(&all);
        let mut archived: Vec<WorkItem> = all
            .iter()
            .filter(|item| item.is_archived())
            .cloned()
            .collect();
        archived.sort_by(|left, right| left.id.cmp(&right.id));
        let ranks = rank_index(&graph);
        Ok(Self {
            graph,
            archived,
            ranks,
        })
    }

    /// The active graph.
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The topological rank of every active item.
    pub fn ranks(&self) -> &BTreeMap<WorkItemId, usize> {
        &self.ranks
    }

    /// Every archived item, in stable-ID order.
    pub fn archived(&self) -> &[WorkItem] {
        &self.archived
    }

    /// One row for an item, deriving readiness from the active graph.
    pub fn row(&self, item: &WorkItem) -> ListedItem {
        let readiness = self
            .graph
            .readiness_any(&item.id)
            .unwrap_or_else(Readiness::closed);
        let children = self.graph.children(&item.id);
        let dependents = self.graph.dependents(&item.id);
        ListedItem {
            readiness,
            children,
            dependents,
            item: item.clone(),
        }
    }

    /// The `_meta` a whole-project read justifies.
    pub fn meta(&self) -> Meta {
        Meta::memory_projection_complete().with_diagnostics(self.graph.diagnostics())
    }
}

/// Which population a collection read asks about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Population {
    /// Every item in the archive range.
    All,
    /// Active, not closed, nothing outstanding.
    Ready,
    /// Active, not closed, at least one outstanding dependency.
    Blocked,
}

impl Population {
    /// Whether this population can ever contain an archived item.
    ///
    /// Readiness is about outstanding work; an archived item is waiting on
    /// nothing, so the ready and blocked populations are active-only even when
    /// the archive range says otherwise.
    pub fn includes_archived(self) -> bool {
        matches!(self, Self::All)
    }

    /// Whether the derived readiness keeps this row.
    pub fn keeps(self, readiness: &Readiness) -> bool {
        match self {
            Self::All => true,
            Self::Ready => readiness.ready,
            Self::Blocked => readiness.blocked,
        }
    }
}

/// A collection request: filters, ordering, and window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// The conjunction.
    pub filters: Filters,
    /// The ordering.
    pub sort: Sort,
    /// The page.
    pub window: Window,
}

impl Query {
    /// The defaults: no filters, `rank` order, 100 rows.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the filters.
    pub fn with_filters(mut self, filters: Filters) -> Self {
        self.filters = filters;
        self
    }

    /// Replace the ordering.
    pub fn with_sort(mut self, sort: Sort) -> Self {
        self.sort = sort;
        self
    }

    /// Set the page limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.window.limit = limit;
        self
    }

    /// Set the page offset.
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.window.offset = offset;
        self
    }

    /// Add a status filter.
    pub fn with_status(mut self, status: Status) -> Self {
        self.filters.statuses.insert(status);
        self
    }

    /// Add a category filter.
    pub fn with_category(mut self, category: CategoryKey) -> Self {
        self.filters.categories.insert(category);
        self
    }

    /// Restrict to one owner.
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.filters.owner = Some(owner.into());
        self
    }

    /// Restrict to one creator.
    pub fn with_creator(mut self, creator: impl Into<String>) -> Self {
        self.filters.creator = Some(creator.into());
        self
    }

    /// Add a custom-field predicate.
    pub fn with_field(mut self, predicate: FieldPredicate) -> Self {
        self.filters.fields.push(predicate);
        self
    }

    /// Add a readiness predicate.
    pub fn with_readiness(mut self, readiness: ReadinessFilter) -> Self {
        self.filters.readiness = Some(readiness);
        self
    }

    /// Select the archive slice.
    pub fn with_archive(mut self, range: ArchiveRange) -> Self {
        self.filters.archive = range;
        self
    }

    /// Set the cheap ID/title filter.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.filters.text = Some(text.into());
        self
    }

    /// Keep only leaves.
    pub fn with_leaves(mut self, leaves: bool) -> Self {
        self.filters.leaves_only = leaves;
        self
    }

    /// Restrict to the children of one item.
    pub fn with_parent(mut self, parent: WorkItemId) -> Self {
        self.filters.parent = Some(parent);
        self
    }

    /// Order by `field`, ascending.
    pub fn order_by(mut self, field: SortField) -> Self {
        self.sort = Sort::of(field);
        self
    }

    /// Reverse the primary ordering.
    pub fn descending(mut self) -> Self {
        self.sort.descending = true;
        self
    }

    /// Validate the custom-field predicates and the sort against the project.
    pub fn validate(&self, config: &Config) -> Result<(), WaymarkError> {
        self.filters.validate(config)?;
        self.sort.validate(config)
    }
}

/// The topological rank of every active item in `graph`.
///
/// The rank is a *level*: every item whose dependencies are all ranked earlier is
/// released together, so a blocker sorts before the item that depends on it and
/// unrelated items do not jump the queue behind it. Within a level items are
/// ordered by stable ID, which makes the order identical on every machine for
/// the same snapshot.
///
/// Items trapped in a dependency cycle never reach indegree zero; they are
/// appended after the last level in stable-ID order, which keeps the ranking
/// total instead of dropping them.
pub fn rank_index(graph: &Graph) -> BTreeMap<WorkItemId, usize> {
    let mut indegree: BTreeMap<WorkItemId, usize> = BTreeMap::new();
    let mut remaining: BTreeSet<WorkItemId> = BTreeSet::new();
    for item in graph.items() {
        let outstanding = item
            .dependencies
            .iter()
            .filter(|target| graph.get(target).is_some())
            .count();
        indegree.insert(item.id.clone(), outstanding);
        remaining.insert(item.id.clone());
    }

    let mut positions: BTreeMap<WorkItemId, usize> = BTreeMap::new();
    loop {
        let level: Vec<WorkItemId> = remaining
            .iter()
            .filter(|id| indegree.get(*id) == Some(&0))
            .cloned()
            .collect();
        if level.is_empty() {
            break;
        }
        for id in &level {
            positions.insert(id.clone(), positions.len());
            remaining.remove(id);
        }
        for id in &level {
            for dependent in graph.dependents(id) {
                if let Some(count) = indegree.get_mut(&dependent) {
                    *count = count.saturating_sub(1);
                }
            }
        }
    }

    // Cycle members, then anything else the levels could not release.
    for id in remaining {
        positions.insert(id, positions.len());
    }
    positions
}

/// Sort rows in place according to `sort`.
///
/// The stable ID is the final tie-break in every mode, ascending, and missing
/// custom values sort after present ones in both directions.
pub fn sort_listed(rows: &mut [ListedItem], sort: &Sort, ranks: &BTreeMap<WorkItemId, usize>) {
    rows.sort_by(|left, right| compare_rows(left, right, sort, ranks));
}

fn compare_rows(
    left: &ListedItem,
    right: &ListedItem,
    sort: &Sort,
    ranks: &BTreeMap<WorkItemId, usize>,
) -> Ordering {
    if let SortField::Custom(key) = &sort.field {
        let left_value = left.item.custom_fields.get(key.as_str());
        let right_value = right.item.custom_fields.get(key.as_str());
        return compare_present(left_value, right_value, sort.descending)
            .then_with(|| left.item.id.cmp(&right.item.id));
    }

    let primary = match &sort.field {
        SortField::Rank => rank_key(left, ranks).cmp(&rank_key(right, ranks)),
        SortField::CreatedAt => left.item.created_at.cmp(&right.item.created_at),
        SortField::UpdatedAt => left.item.updated_at.cmp(&right.item.updated_at),
        SortField::Title => left.item.title.cmp(&right.item.title),
        SortField::Status => left
            .item
            .status
            .rank_group()
            .cmp(&right.item.status.rank_group()),
        SortField::Custom(_) => unreachable!("handled above"),
    };
    let primary = if sort.descending {
        primary.reverse()
    } else {
        primary
    };
    primary.then_with(|| left.item.id.cmp(&right.item.id))
}

fn rank_key(row: &ListedItem, ranks: &BTreeMap<WorkItemId, usize>) -> (u8, usize) {
    (
        row.item.status.rank_group(),
        ranks.get(&row.item.id).copied().unwrap_or(usize::MAX),
    )
}

/// Compare two optional custom values with missing-last semantics.
fn compare_present(
    left: Option<&FieldValue>,
    right: Option<&FieldValue>,
    descending: bool,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => {
            let order = compare_scalars(left, right).unwrap_or(Ordering::Equal);
            if descending {
                order.reverse()
            } else {
                order
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// The fixed field vocabulary a collection result can project.
///
/// Frontmatter and derived columns are both in the vocabulary; the renderer
/// keeps the established nested `item` shape for the persisted fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FieldName {
    /// Format generation.
    Schema,
    /// Stable ID.
    Id,
    /// Title.
    Title,
    /// Category key.
    Category,
    /// Lifecycle state.
    Status,
    /// Owner.
    Owner,
    /// Creator.
    Creator,
    /// Creation instant.
    CreatedAt,
    /// Last-change instant.
    UpdatedAt,
    /// Close instant.
    ClosedAt,
    /// Parent ID.
    Parent,
    /// Dependency IDs.
    Dependencies,
    /// Soft-link IDs.
    SoftLinks,
    /// Custom-field map.
    CustomFields,
    /// Archive instant.
    ArchivedAt,
    /// Archive reason.
    ArchiveReason,
    /// Derived readiness.
    Ready,
    /// Derived blockage.
    Blocked,
    /// The dependency targets that block the item.
    BlockedBy,
    /// Active children.
    Children,
    /// Active dependents.
    Dependents,
}

impl FieldName {
    /// Every name, in a stable order suitable for documentation and errors.
    pub const ALL: &'static [FieldName] = &[
        Self::Schema,
        Self::Id,
        Self::Title,
        Self::Category,
        Self::Status,
        Self::Owner,
        Self::Creator,
        Self::CreatedAt,
        Self::UpdatedAt,
        Self::ClosedAt,
        Self::Parent,
        Self::Dependencies,
        Self::SoftLinks,
        Self::CustomFields,
        Self::ArchivedAt,
        Self::ArchiveReason,
        Self::Ready,
        Self::Blocked,
        Self::BlockedBy,
        Self::Children,
        Self::Dependents,
    ];

    /// The identity columns a `--compact` row keeps.
    pub const COMPACT: &'static [FieldName] = &[
        Self::Id,
        Self::Title,
        Self::Category,
        Self::Status,
        Self::Owner,
    ];

    /// The wire and CLI literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Id => "id",
            Self::Title => "title",
            Self::Category => "category",
            Self::Status => "status",
            Self::Owner => "owner",
            Self::Creator => "creator",
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
            Self::ClosedAt => "closed_at",
            Self::Parent => "parent",
            Self::Dependencies => "dependencies",
            Self::SoftLinks => "soft_links",
            Self::CustomFields => "custom_fields",
            Self::ArchivedAt => "archived_at",
            Self::ArchiveReason => "archive_reason",
            Self::Ready => "ready",
            Self::Blocked => "blocked",
            Self::BlockedBy => "blocked_by",
            Self::Children => "children",
            Self::Dependents => "dependents",
        }
    }

    /// Parse the wire literal.
    pub fn parse(input: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|field| field.as_str() == input.trim())
    }

    /// Whether this is a persisted frontmatter field (as opposed to a derived
    /// column).
    pub fn is_frontmatter(self) -> bool {
        !matches!(
            self,
            Self::Ready | Self::Blocked | Self::BlockedBy | Self::Children | Self::Dependents
        )
    }
}

/// A fixed or named field selection plus the compact mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Projection {
    /// The named set, or `None` for every field.
    pub fields: Option<BTreeSet<FieldName>>,
    /// Render only the identity columns and omit empty values.
    pub compact: bool,
}

impl Projection {
    /// Every field, full rendering.
    pub fn all() -> Self {
        Self::default()
    }

    /// Select an explicit set.
    pub fn of(fields: impl IntoIterator<Item = FieldName>) -> Self {
        Self {
            fields: Some(fields.into_iter().collect()),
            compact: false,
        }
    }

    /// Render the compact row.
    pub fn compact() -> Self {
        Self {
            fields: None,
            compact: true,
        }
    }

    /// Parse a comma-separated `--fields` value.
    pub fn parse_list(input: &str) -> Result<Self, WaymarkError> {
        let mut fields = BTreeSet::new();
        for token in input.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            let field = FieldName::parse(token).ok_or_else(|| {
                invalid(
                    "fields",
                    format!(
                        "{token:?} is not a projected field; expected one of {}",
                        FieldName::ALL
                            .iter()
                            .map(|field| field.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
            })?;
            fields.insert(field);
        }
        if fields.is_empty() {
            return Err(invalid("fields", "at least one field must be named"));
        }
        Ok(Self {
            fields: Some(fields),
            compact: false,
        })
    }

    /// Whether `field` is rendered.
    pub fn includes(&self, field: FieldName) -> bool {
        match (&self.fields, self.compact) {
            (Some(fields), _) => fields.contains(&field),
            (None, true) => FieldName::COMPACT.contains(&field),
            (None, false) => true,
        }
    }
}

/// Render one row with `projection` applied.
///
/// The shape is fixed: persisted fields live under `item`, derived columns are
/// top-level, `id` and `status` are always present, the body is never included,
/// and unset values are omitted. `compact` additionally drops every derived
/// column unless `--fields` names it explicitly.
pub fn projected_row(row: &ListedItem, projection: &Projection) -> Value {
    let mut item = Map::new();
    for field in FieldName::ALL.iter().copied() {
        if !field.is_frontmatter() || !projection.includes(field) {
            continue;
        }
        let value = frontmatter_value(&row.item, field);
        if value.is_null() {
            continue;
        }
        item.insert(field.as_str().to_owned(), value);
    }

    let mut object = Map::new();
    object.insert("item".to_owned(), Value::Object(item));
    object.insert("id".to_owned(), json!(row.item.id.as_str()));
    object.insert("status".to_owned(), json!(row.item.status.as_str()));
    for field in FieldName::ALL.iter().copied() {
        if field.is_frontmatter() || !projection.includes(field) {
            continue;
        }
        let value = derived_value(row, field);
        if projection.compact && projection.fields.is_none() && value.is_null() {
            continue;
        }
        object.insert(field.as_str().to_owned(), value);
    }
    Value::Object(object)
}

fn frontmatter_value(item: &WorkItem, field: FieldName) -> Value {
    match field {
        FieldName::Schema => json!(item.schema),
        FieldName::Id => json!(item.id.as_str()),
        FieldName::Title => json!(item.title),
        FieldName::Category => json!(item.category.as_str()),
        FieldName::Status => json!(item.status.as_str()),
        FieldName::Owner => json!(item.owner),
        FieldName::Creator => json!(item.creator),
        FieldName::CreatedAt => json!(item.created_at.to_string()),
        FieldName::UpdatedAt => json!(item.updated_at.to_string()),
        FieldName::ClosedAt => json!(item.closed_at.map(|value| value.to_string())),
        FieldName::Parent => json!(item.parent.as_ref().map(|id| id.as_str())),
        FieldName::Dependencies => optional_ids(&item.dependencies),
        FieldName::SoftLinks => optional_ids(&item.soft_links),
        FieldName::CustomFields => {
            if item.custom_fields.is_empty() {
                Value::Null
            } else {
                json!(item.custom_fields)
            }
        }
        FieldName::ArchivedAt => json!(item.archived_at.map(|value| value.to_string())),
        FieldName::ArchiveReason => json!(item.archive_reason),
        FieldName::Ready
        | FieldName::Blocked
        | FieldName::BlockedBy
        | FieldName::Children
        | FieldName::Dependents => Value::Null,
    }
}

fn optional_ids(ids: &[WorkItemId]) -> Value {
    if ids.is_empty() {
        Value::Null
    } else {
        json!(ids.iter().map(|id| id.as_str()).collect::<Vec<_>>())
    }
}

fn derived_value(row: &ListedItem, field: FieldName) -> Value {
    match field {
        FieldName::Ready => json!(row.readiness.ready),
        FieldName::Blocked => json!(row.readiness.blocked),
        FieldName::BlockedBy => json!(row
            .readiness
            .blocked_by
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()),
        FieldName::Children => json!(row
            .children
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()),
        FieldName::Dependents => {
            json!(row
                .dependents
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>())
        }
        _ => Value::Null,
    }
}

/// Build the `_meta` block for a whole-project read.
///
/// Kept here so `list`, `search`, and `stats` cannot disagree about what a
/// complete read claims; trees reuse it too.
pub fn complete_meta(diagnostics: Vec<Finding>) -> Meta {
    Meta::memory_projection_complete().with_diagnostics(diagnostics)
}
