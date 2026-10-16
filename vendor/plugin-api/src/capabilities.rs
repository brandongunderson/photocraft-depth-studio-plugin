use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{ApiError, ErrorCode, HOST_API_VERSION, Result, identifier, invalid};

pub const MAX_CAPABILITIES: usize = 32;
pub const MAX_CAPABILITY_VERSION: u16 = 1024;

/// Extensible capability identifier. Unknown optional IDs survive parsing and negotiate absent.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct CapabilityId(#[cfg_attr(feature = "schema", schemars(length(min = 1, max = 96), regex(pattern = "^[a-z0-9][a-z0-9._-]*$")))] String);

impl CapabilityId {
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        if !identifier(&id, 96) {
            return Err(invalid("capabilities.id", "invalid capability ID"));
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn known(&self) -> Option<KnownCapability> {
        KnownCapability::ALL.into_iter().find(|id| id.as_str() == self.0)
    }
}

impl<'de> Deserialize<'de> for CapabilityId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Capabilities implemented by the initial host adapter. Adding an ID does not change panel ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum KnownCapability {
    #[serde(rename = "host.commands.invoke")]
    CommandsInvoke,
    #[serde(rename = "host.document.read")]
    DocumentRead,
    #[serde(rename = "host.document.edit")]
    DocumentEdit,
    #[serde(rename = "host.events")]
    Events,
    #[serde(rename = "host.ui.views")]
    UiViews,
}

impl KnownCapability {
    pub const ALL: [Self; 5] = [Self::CommandsInvoke, Self::DocumentRead, Self::DocumentEdit, Self::Events, Self::UiViews];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CommandsInvoke => "host.commands.invoke",
            Self::DocumentRead => "host.document.read",
            Self::DocumentEdit => "host.document.edit",
            Self::Events => "host.events",
            Self::UiViews => "host.ui.views",
        }
    }
}

impl From<KnownCapability> for CapabilityId {
    fn from(value: KnownCapability) -> Self {
        Self(value.as_str().into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityRequirement {
    pub id: CapabilityId,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 1024)))]
    pub min_version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 1024)))]
    pub max_version: Option<u16>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capabilities {
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(length(max = 32)))]
    pub required: Vec<CapabilityRequirement>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(length(max = 32)))]
    pub optional: Vec<CapabilityRequirement>,
}

impl Capabilities {
    pub fn is_empty(&self) -> bool {
        self.required.is_empty() && self.optional.is_empty()
    }

    pub fn validate(&self) -> Result<()> {
        if self.required.len().saturating_add(self.optional.len()) > MAX_CAPABILITIES {
            return Err(invalid("capabilities", "at most 32 total required and optional capabilities are supported"));
        }
        let mut seen = BTreeSet::new();
        for item in self.required.iter().chain(&self.optional) {
            if !identifier(item.id.as_str(), 96) || !seen.insert(&item.id) {
                return Err(invalid("capabilities", "invalid or duplicate capability ID"));
            }
            if !(1..=MAX_CAPABILITY_VERSION).contains(&item.min_version)
                || item.max_version.is_some_and(|max| max < item.min_version || max > MAX_CAPABILITY_VERSION)
            {
                return Err(invalid("capabilities", "capability versions must be an ordered range within 1..=1024"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilitySupport {
    pub id: CapabilityId,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 1024)))]
    pub version: u16,
}

/// Public discovery metadata, containing no authority or credentials.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostApi {
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 1)))]
    pub version: u16,
    #[cfg_attr(feature = "schema", schemars(length(max = 32)))]
    pub capabilities: Vec<CapabilitySupport>,
}

impl Default for HostApi {
    fn default() -> Self {
        Self { version: HOST_API_VERSION, capabilities: Vec::new() }
    }
}

impl HostApi {
    pub fn validate(&self) -> Result<()> {
        if self.version != HOST_API_VERSION {
            return Err(ApiError::at(ErrorCode::IncompatibleApi, "hostApi.version", "unsupported host metadata version"));
        }
        // Empty requirements still validate the advertised versions and uniqueness.
        negotiate(&Capabilities::default(), &self.capabilities)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiatedCapabilities {
    pub available: Vec<CapabilitySupport>,
    pub unavailable_optional: Vec<CapabilityId>,
}

/// Select the exact supported capability version. Discovery never grants permission.
pub fn negotiate(requirements: &Capabilities, supported: &[CapabilitySupport]) -> Result<NegotiatedCapabilities> {
    requirements.validate()?;
    let mut seen = BTreeSet::new();
    if supported.len() > MAX_CAPABILITIES || supported.iter().any(|item| !seen.insert(&item.id) || !(1..=MAX_CAPABILITY_VERSION).contains(&item.version)) {
        return Err(invalid("host.capabilities", "invalid or duplicate advertised capabilities"));
    }
    let mut result = NegotiatedCapabilities::default();
    for (required, list) in [(true, &requirements.required), (false, &requirements.optional)] {
        for requested in list {
            let offer = supported.iter().find(|item| item.id == requested.id);
            if let Some(offer) = offer
                && offer.version >= requested.min_version
                && requested.max_version.is_none_or(|max| offer.version <= max)
            {
                result.available.push(offer.clone());
            } else if required {
                return Err(ApiError {
                    code: if offer.is_some() { ErrorCode::UnsupportedVersion } else { ErrorCode::MissingCapability },
                    message: format!("required capability {} is unavailable at the requested version", requested.id),
                    details: json!({"capability":requested.id,"minVersion":requested.min_version,"maxVersion":requested.max_version,"availableVersion":offer.map(|item|item.version)}),
                });
            } else {
                result.unavailable_optional.push(requested.id.clone());
            }
        }
    }
    Ok(result)
}
