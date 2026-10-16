use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Result, event_identifier, identifier, invalid, json_size, label};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contributions {
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(length(max = 32)))]
    pub commands: Vec<CommandContribution>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(length(max = 8)))]
    pub inspectors: Vec<InspectorContribution>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(length(max = 16)))]
    pub subscriptions: Vec<EventSubscription>,
}

impl Contributions {
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty() && self.inspectors.is_empty() && self.subscriptions.is_empty()
    }

    /// Validate namespaced identifiers and bounded descriptors without activating plugin code.
    pub fn validate(&self, plugin_id: &str) -> Result<()> {
        if !identifier(plugin_id, 64) {
            return Err(invalid("id", "invalid plugin owner ID"));
        }
        if self.commands.len() > 32 || self.inspectors.len() > 8 || self.subscriptions.len() > 16 {
            return Err(invalid("contributes", "contribution limits are 32 commands, 8 inspectors and 16 subscriptions"));
        }
        let prefix = format!("{plugin_id}.");
        let mut seen = BTreeSet::new();
        let mut check_id = |id: &str| -> Result<()> {
            if !identifier(id, 128) || !id.strip_prefix(&prefix).is_some_and(|local| identifier(local, 63)) || !seen.insert(id.to_owned()) {
                return Err(invalid("contributes.id", "contribution IDs must be unique and prefixed by the plugin ID followed by a dot"));
            }
            Ok(())
        };
        for command in &self.commands {
            check_id(&command.id)?;
            if !label(&command.label, 128) || !event_identifier(&command.event) {
                return Err(invalid("contributes.commands", "invalid command label or event handler"));
            }
            json_size(&Value::Object(command.params.clone()), 8192, "contributes.commands.params")?;
        }
        for inspector in &self.inspectors {
            check_id(&inspector.id)?;
            if !label(&inspector.label, 128) || !event_identifier(&inspector.event) {
                return Err(invalid("contributes.inspectors", "invalid inspector label or event handler"));
            }
        }
        for subscription in &self.subscriptions {
            check_id(&subscription.id)?;
            if !event_identifier(&subscription.handler) {
                return Err(invalid("contributes.subscriptions", "invalid subscription handler"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandContribution {
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 128)))]
    pub id: String,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 128)))]
    pub label: String,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 64), regex(pattern = "^[A-Za-z0-9][A-Za-z0-9_]*$")))]
    pub event: String,
    #[serde(default)]
    pub params: Map<String, Value>,
    #[serde(default)]
    pub needs_document: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectorContribution {
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 128)))]
    pub id: String,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 128)))]
    pub label: String,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 64), regex(pattern = "^[A-Za-z0-9][A-Za-z0-9_]*$")))]
    pub event: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum EventKind {
    #[serde(rename = "document.changed")]
    DocumentChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventSubscription {
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 128)))]
    pub id: String,
    pub event: EventKind,
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 64), regex(pattern = "^[A-Za-z0-9][A-Za-z0-9_]*$")))]
    pub handler: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectorView {
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(length(max = 128)))]
    pub title: String,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(length(max = 64)))]
    pub rows: Vec<InspectorRow>,
}

impl InspectorView {
    pub fn validate(&self) -> Result<()> {
        if self.rows.len() > 64 || self.title.chars().count() > 128 || (!self.title.is_empty() && !label(&self.title, 128)) {
            return Err(invalid("view", "inspector view exceeds its title or row budget"));
        }
        for row in &self.rows {
            if !label(&row.label, 128) || row.value.chars().count() > 1024 || row.value.chars().any(char::is_control) {
                return Err(invalid("view.rows", "inspector rows need printable labels and values of at most 1024 characters"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectorRow {
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 128)))]
    pub label: String,
    #[cfg_attr(feature = "schema", schemars(length(max = 1024)))]
    pub value: String,
}
