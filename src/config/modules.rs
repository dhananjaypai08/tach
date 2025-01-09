use super::root_module::ROOT_MODULE_SENTINEL_TAG;
use super::utils::*;
use pyo3::prelude::*;
use serde::ser::{Error, SerializeSeq, SerializeStruct};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
#[pyclass(get_all, module = "tach.extension")]
pub struct DependencyConfig {
    pub path: String,
    pub deprecated: bool,
}

impl Serialize for DependencyConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Should actually express that all fields are default except for path
        if !self.deprecated {
            serializer.serialize_str(&self.path)
        } else {
            let mut state = serializer.serialize_struct("DependencyConfig", 2)?;
            state.serialize_field("path", &self.path)?;
            state.serialize_field("deprecated", &self.deprecated)?;
            state.end()
        }
    }
}

impl DependencyConfig {
    pub fn from_deprecated_path(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            deprecated: true,
        }
    }
    pub fn from_path(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            deprecated: false,
        }
    }
}
struct DependencyConfigVisitor;

impl<'de> de::Visitor<'de> for DependencyConfigVisitor {
    type Value = DependencyConfig;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string or map")
    }

    fn visit_str<E>(self, value: &str) -> Result<DependencyConfig, E>
    where
        E: de::Error,
    {
        Ok(DependencyConfig {
            path: value.to_string(),
            ..Default::default()
        })
    }

    // Unfortunately don't have the derived Deserialize for this
    fn visit_map<M>(self, mut map: M) -> Result<DependencyConfig, M::Error>
    where
        M: de::MapAccess<'de>,
    {
        let mut path = None;
        let mut deprecated = false;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "path" => {
                    path = {
                        if path.is_some() {
                            return Err(de::Error::duplicate_field("path"));
                        }
                        Some(map.next_value()?)
                    }
                }
                "deprecated" => {
                    if deprecated {
                        return Err(de::Error::duplicate_field("deprecated"));
                    }
                    deprecated = map.next_value()?;
                }
                _ => {
                    return Err(de::Error::unknown_field(&key, &["path", "deprecated"]));
                }
            }
        }

        let path = path.ok_or_else(|| de::Error::missing_field("path"))?;

        Ok(DependencyConfig { path, deprecated })
    }
}

impl<'de> Deserialize<'de> for DependencyConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(DependencyConfigVisitor)
    }
}

pub fn default_visibility() -> Vec<String> {
    global_visibility()
}

pub fn is_default_visibility(value: &Vec<String>) -> bool {
    value == &default_visibility()
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[pyclass(get_all, eq, module = "tach.extension")]
pub struct ModuleConfig {
    pub path: String,
    #[serde(default)]
    #[pyo3(set)]
    pub depends_on: Vec<DependencyConfig>,
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(
        default = "default_visibility",
        skip_serializing_if = "is_default_visibility"
    )]
    pub visibility: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub utility: bool,
    // TODO: Remove this in a future version
    // This will be deserialized from old config,
    // but auto-migrated to interfaces internally.
    // This means we don't want to serialize it.
    #[serde(default, skip_serializing)]
    pub strict: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub unchecked: bool,
    // Hidden field to track grouping
    // Unfortunately marked as public due to test fixtures constructing struct literals
    #[doc(hidden)]
    pub group_id: Option<usize>,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            path: Default::default(),
            depends_on: Default::default(),
            layer: Default::default(),
            visibility: default_visibility(),
            utility: Default::default(),
            strict: Default::default(),
            unchecked: Default::default(),
            group_id: Default::default(),
        }
    }
}

#[pymethods]
impl ModuleConfig {
    #[new]
    pub fn new(path: &str, strict: bool) -> Self {
        Self {
            path: path.to_string(),
            depends_on: vec![],
            layer: None,
            visibility: default_visibility(),
            utility: false,
            strict,
            unchecked: false,
            group_id: None,
        }
    }

    pub fn with_no_dependencies(&self) -> Self {
        let mut new_module = self.clone();
        new_module.depends_on = vec![];
        new_module
    }

    #[staticmethod]
    pub fn new_root_config() -> Self {
        Self::new(ROOT_MODULE_SENTINEL_TAG, false)
    }
    pub fn mod_path(&self) -> String {
        if self.path == ROOT_MODULE_SENTINEL_TAG {
            return ".".to_string();
        }
        self.path.clone()
    }
}

#[derive(Serialize, Deserialize)]
struct BulkModule {
    paths: Vec<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    depends_on: Vec<DependencyConfig>,
    #[serde(default)]
    layer: Option<String>,
    #[serde(
        default = "default_visibility",
        skip_serializing_if = "is_default_visibility"
    )]
    visibility: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    utility: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    unchecked: bool,
}

impl TryFrom<&[&ModuleConfig]> for BulkModule {
    type Error = String;

    fn try_from(modules: &[&ModuleConfig]) -> Result<Self, Self::Error> {
        if modules.is_empty() {
            return Err("Cannot create BulkModule from empty slice".to_string());
        }

        let first = modules[0];
        let mut bulk = BulkModule {
            paths: modules.iter().map(|m| m.path.clone()).collect(),
            depends_on: Vec::new(),
            layer: first.layer.clone(),
            visibility: first.visibility.clone(),
            utility: first.utility,
            unchecked: first.unchecked,
        };

        let mut unique_deps: HashSet<DependencyConfig> = HashSet::new();
        for module in modules {
            unique_deps.extend(module.depends_on.clone());

            // Validate that other fields match the first module
            if module.layer != first.layer {
                return Err(format!(
                    "Inconsistent layer in bulk module group for path {}",
                    module.path
                ));
            }
            if module.visibility != first.visibility {
                return Err(format!(
                    "Inconsistent visibility in bulk module group for path {}",
                    module.path
                ));
            }
            if module.utility != first.utility {
                return Err(format!(
                    "Inconsistent utility setting in bulk module group for path {}",
                    module.path
                ));
            }
            if module.strict != first.strict {
                return Err(format!(
                    "Inconsistent strict setting in bulk module group for path {}",
                    module.path
                ));
            }
            if module.unchecked != first.unchecked {
                return Err(format!(
                    "Inconsistent unchecked setting in bulk module group for path {}",
                    module.path
                ));
            }
        }

        bulk.depends_on = unique_deps.into_iter().collect();
        Ok(bulk)
    }
}

pub fn serialize_modules<S>(modules: &Vec<ModuleConfig>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut grouped: HashMap<Option<usize>, Vec<&ModuleConfig>> = HashMap::new();

    for module in modules {
        grouped.entry(module.group_id).or_default().push(module);
    }

    let mut seq = serializer.serialize_seq(Some(grouped.len()))?;

    for (group_key, group_modules) in grouped {
        match group_key {
            // Single modules (no group)
            None => {
                for module in group_modules {
                    seq.serialize_element(module)?;
                }
            }
            // Grouped modules
            Some(_) => {
                if !group_modules.is_empty() {
                    let bulk =
                        BulkModule::try_from(group_modules.as_slice()).map_err(S::Error::custom)?;
                    seq.serialize_element(&bulk)?;
                }
            }
        }
    }

    seq.end()
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ModuleConfigOrBulk {
    Single(ModuleConfig),
    Bulk(BulkModule),
}

pub fn deserialize_modules<'de, D>(deserializer: D) -> Result<Vec<ModuleConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let configs: Vec<ModuleConfigOrBulk> = Vec::deserialize(deserializer)?;

    Ok(configs
        .into_iter()
        .enumerate()
        .flat_map(|(i, config)| match config {
            ModuleConfigOrBulk::Single(module) => vec![module],
            ModuleConfigOrBulk::Bulk(bulk) => bulk
                .paths
                .into_iter()
                .map(|path| ModuleConfig {
                    path,
                    depends_on: bulk.depends_on.clone(),
                    layer: bulk.layer.clone(),
                    visibility: bulk.visibility.clone(),
                    utility: bulk.utility,
                    strict: false,
                    unchecked: bulk.unchecked,
                    group_id: Some(i),
                })
                .collect(),
        })
        .collect())
}
