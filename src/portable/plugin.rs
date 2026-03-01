//! Plugin Architecture for Parsanol
//!
//! This module provides a plugin system for extending parsanol with third-party
//! functionality. Plugins can register custom atoms, transforms, and other
//! extensions.
//!
//! # Overview
//!
//! The plugin system allows third-party crates to extend parsanol's functionality
//! by implementing the [`ParsanolPlugin`] trait and registering their extensions.
//!
//! # Example
//!
//! ```
//! use parsanol::portable::plugin::{ParsanolPlugin, PluginRegistry, AtomRegistry, TransformRegistry};
//!
//! /// A plugin that adds JSON parsing support
//! struct JsonPlugin;
//!
//! impl ParsanolPlugin for JsonPlugin {
//!     fn name(&self) -> &str {
//!         "json"
//!     }
//!
//!     fn version(&self) -> &str {
//!         "1.0.0"
//!     }
//!
//!     fn description(&self) -> &str {
//!         "JSON parsing extensions for parsanol"
//!     }
//!
//!     fn register_atoms(&self, registry: &mut AtomRegistry) {
//!         // Register custom atoms for JSON parsing
//!         // registry.register("json_string", ...);
//!     }
//!
//!     fn register_transforms(&self, registry: &mut TransformRegistry) {
//!         // Register transforms for JSON AST conversion
//!         // registry.register("json_to_value", ...);
//!     }
//! }
//!
//! // Register the plugin
//! let mut registry = PluginRegistry::new();
//! registry.register_plugin(Box::new(JsonPlugin));
//! ```

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::custom::CustomAtom;

// ============================================================================
// Plugin Trait
// ============================================================================

/// Trait for parsanol plugins
///
/// Implement this trait to create a plugin that extends parsanol's functionality.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` because they may be called from
/// multiple threads.
pub trait ParsanolPlugin: Send + Sync {
    /// Get the plugin name (unique identifier)
    fn name(&self) -> &str;

    /// Get the plugin version
    fn version(&self) -> &str {
        "0.0.0"
    }

    /// Get a description of the plugin
    fn description(&self) -> &str {
        ""
    }

    /// Register custom atoms with the registry
    ///
    /// Override this method to add custom parsing atoms.
    fn register_atoms(&self, _registry: &mut AtomRegistry) {}

    /// Register transforms with the registry
    ///
    /// Override this method to add AST transformation functions.
    fn register_transforms(&self, _registry: &mut TransformRegistry) {}

    /// Called when the plugin is loaded
    ///
    /// Override this method to perform initialization when the plugin is loaded.
    fn on_load(&self) {}

    /// Called when the plugin is unloaded
    ///
    /// Override this method to perform cleanup when the plugin is unloaded.
    fn on_unload(&self) {}
}

// ============================================================================
// Atom Registry
// ============================================================================

/// Registry for custom atoms provided by plugins
pub struct AtomRegistry {
    /// Custom atoms by name
    atoms: HashMap<String, AtomEntry>,
}

/// Information about a registered atom
pub struct AtomInfo {
    /// The plugin that registered this atom
    pub plugin: String,
    /// Description of the atom
    pub description: String,
}

/// Entry in the atom registry
struct AtomEntry {
    /// The custom atom implementation
    atom: Box<dyn CustomAtom>,
    /// The plugin that registered this atom
    plugin: String,
    /// Description of the atom
    description: String,
}

impl AtomRegistry {
    /// Create a new empty atom registry
    pub fn new() -> Self {
        Self {
            atoms: HashMap::new(),
        }
    }

    /// Register a custom atom
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for the atom
    /// * `atom` - The custom atom implementation
    /// * `plugin` - Name of the plugin registering this atom
    ///
    /// # Returns
    ///
    /// `true` if the atom was registered, `false` if an atom with that name
    /// already exists.
    pub fn register(
        &mut self,
        name: &str,
        atom: Box<dyn CustomAtom>,
        plugin: &str,
    ) -> bool {
        if self.atoms.contains_key(name) {
            return false;
        }

        let description = atom.description().to_string();
        self.atoms.insert(
            name.to_string(),
            AtomEntry {
                atom,
                plugin: plugin.to_string(),
                description,
            },
        );
        true
    }

    /// Get information about a custom atom by name
    pub fn get(&self, name: &str) -> Option<AtomInfo> {
        self.atoms.get(name).map(|entry| AtomInfo {
            plugin: entry.plugin.clone(),
            description: entry.description.clone(),
        })
    }

    /// Check if an atom exists
    pub fn contains(&self, name: &str) -> bool {
        self.atoms.contains_key(name)
    }

    /// Get the number of registered atoms
    pub fn len(&self) -> usize {
        self.atoms.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }

    /// List all registered atoms
    pub fn list(&self) -> Vec<(&str, &str, &str)> {
        self.atoms
            .iter()
            .map(|(name, entry)| (name.as_str(), entry.plugin.as_str(), entry.description.as_str()))
            .collect()
    }

    /// Unregister an atom
    ///
    /// # Returns
    ///
    /// `true` if the atom was removed, `false` if it didn't exist.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.atoms.remove(name).is_some()
    }

    /// Clear all registered atoms
    pub fn clear(&mut self) {
        self.atoms.clear();
    }
}

impl Default for AtomRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Transform Registry
// ============================================================================

/// Registry for transforms provided by plugins
pub struct TransformRegistry {
    /// Transform functions by name
    transforms: HashMap<String, TransformEntry>,
}

/// Entry in the transform registry
pub struct TransformEntry {
    /// The plugin that registered this transform
    pub plugin: String,
    /// Description of the transform
    pub description: String,
    /// Transform function metadata (name patterns it handles)
    pub patterns: Vec<String>,
}

impl TransformRegistry {
    /// Create a new empty transform registry
    pub fn new() -> Self {
        Self {
            transforms: HashMap::new(),
        }
    }

    /// Register a transform
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for the transform
    /// * `plugin` - Name of the plugin registering this transform
    /// * `description` - Description of what the transform does
    /// * `patterns` - Name patterns this transform handles (e.g., ["number", "string"])
    pub fn register(
        &mut self,
        name: &str,
        plugin: &str,
        description: &str,
        patterns: Vec<String>,
    ) -> bool {
        if self.transforms.contains_key(name) {
            return false;
        }

        self.transforms.insert(
            name.to_string(),
            TransformEntry {
                plugin: plugin.to_string(),
                description: description.to_string(),
                patterns,
            },
        );
        true
    }

    /// Get a transform by name
    pub fn get(&self, name: &str) -> Option<&TransformEntry> {
        self.transforms.get(name)
    }

    /// Check if a transform exists
    pub fn contains(&self, name: &str) -> bool {
        self.transforms.contains_key(name)
    }

    /// Get the number of registered transforms
    pub fn len(&self) -> usize {
        self.transforms.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.transforms.is_empty()
    }

    /// List all registered transforms
    pub fn list(&self) -> Vec<(&str, &str, &str)> {
        self.transforms
            .iter()
            .map(|(name, entry)| (name.as_str(), entry.plugin.as_str(), entry.description.as_str()))
            .collect()
    }

    /// Unregister a transform
    pub fn unregister(&mut self, name: &str) -> bool {
        self.transforms.remove(name).is_some()
    }

    /// Clear all registered transforms
    pub fn clear(&mut self) {
        self.transforms.clear();
    }
}

impl Default for TransformRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Plugin Registry
// ============================================================================

/// Global registry for plugins
static PLUGIN_REGISTRY: OnceLock<Mutex<PluginRegistry>> = OnceLock::new();

/// Registry for managing plugins
pub struct PluginRegistry {
    /// Registered plugins by name
    plugins: HashMap<String, Box<dyn ParsanolPlugin>>,
    /// Atom registry
    atoms: AtomRegistry,
    /// Transform registry
    transforms: TransformRegistry,
}

impl PluginRegistry {
    /// Create a new empty plugin registry
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            atoms: AtomRegistry::new(),
            transforms: TransformRegistry::new(),
        }
    }

    /// Register a plugin
    ///
    /// # Arguments
    ///
    /// * `plugin` - The plugin to register
    ///
    /// # Returns
    ///
    /// `true` if the plugin was registered, `false` if a plugin with that
    /// name already exists.
    pub fn register_plugin(&mut self, plugin: Box<dyn ParsanolPlugin>) -> bool {
        let name = plugin.name().to_string();
        if self.plugins.contains_key(&name) {
            return false;
        }

        // Call on_load before registration
        plugin.on_load();

        // Register atoms and transforms
        plugin.register_atoms(&mut self.atoms);
        plugin.register_transforms(&mut self.transforms);

        self.plugins.insert(name, plugin);
        true
    }

    /// Unregister a plugin by name
    ///
    /// # Returns
    ///
    /// `true` if the plugin was removed, `false` if it didn't exist.
    pub fn unregister_plugin(&mut self, name: &str) -> bool {
        if let Some(plugin) = self.plugins.remove(name) {
            plugin.on_unload();
            true
        } else {
            false
        }
    }

    /// Check if a plugin is registered
    pub fn has_plugin(&self, name: &str) -> bool {
        self.plugins.contains_key(name)
    }

    /// Get plugin info
    pub fn get_plugin_info(&self, name: &str) -> Option<PluginInfo> {
        self.plugins.get(name).map(|p| PluginInfo {
            name: p.name().to_string(),
            version: p.version().to_string(),
            description: p.description().to_string(),
        })
    }

    /// Get the atom registry
    pub fn atoms(&self) -> &AtomRegistry {
        &self.atoms
    }

    /// Get the transform registry
    pub fn transforms(&self) -> &TransformRegistry {
        &self.transforms
    }

    /// Get mutable access to the atom registry
    pub fn atoms_mut(&mut self) -> &mut AtomRegistry {
        &mut self.atoms
    }

    /// Get mutable access to the transform registry
    pub fn transforms_mut(&mut self) -> &mut TransformRegistry {
        &mut self.transforms
    }

    /// List all registered plugins
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .values()
            .map(|p| PluginInfo {
                name: p.name().to_string(),
                version: p.version().to_string(),
                description: p.description().to_string(),
            })
            .collect()
    }

    /// Get the number of registered plugins
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Clear all plugins
    pub fn clear(&mut self) {
        // Call on_unload for each plugin
        for plugin in self.plugins.values() {
            plugin.on_unload();
        }
        self.plugins.clear();
        self.atoms.clear();
        self.transforms.clear();
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a plugin
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// Plugin name
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: String,
}

// ============================================================================
// Global Registry Functions
// ============================================================================

/// Get or initialize the global plugin registry
fn get_global_registry() -> &'static Mutex<PluginRegistry> {
    PLUGIN_REGISTRY.get_or_init(|| Mutex::new(PluginRegistry::new()))
}

/// Register a plugin with the global registry
///
/// # Example
///
/// ```
/// use parsanol::portable::plugin::{ParsanolPlugin, register_plugin};
///
/// struct MyPlugin;
/// impl ParsanolPlugin for MyPlugin {
///     fn name(&self) -> &str { "my_plugin" }
/// }
///
/// register_plugin(Box::new(MyPlugin));
/// ```
pub fn register_plugin(plugin: Box<dyn ParsanolPlugin>) -> bool {
    let registry = get_global_registry();
    let mut guard = registry.lock().unwrap();
    guard.register_plugin(plugin)
}

/// Unregister a plugin from the global registry
pub fn unregister_plugin(name: &str) -> bool {
    let registry = get_global_registry();
    let mut guard = registry.lock().unwrap();
    guard.unregister_plugin(name)
}

/// Check if a plugin is registered in the global registry
pub fn has_plugin(name: &str) -> bool {
    let registry = get_global_registry();
    let guard = registry.lock().unwrap();
    guard.has_plugin(name)
}

/// Get plugin info from the global registry
pub fn get_plugin_info(name: &str) -> Option<PluginInfo> {
    let registry = get_global_registry();
    let guard = registry.lock().unwrap();
    guard.get_plugin_info(name)
}

/// List all plugins in the global registry
pub fn list_plugins() -> Vec<PluginInfo> {
    let registry = get_global_registry();
    let guard = registry.lock().unwrap();
    guard.list_plugins()
}

/// Get the number of plugins in the global registry
pub fn plugin_count() -> usize {
    let registry = get_global_registry();
    let guard = registry.lock().unwrap();
    guard.len()
}

/// Clear all plugins from the global registry
///
/// # Warning
///
/// This is intended for testing purposes only.
pub fn clear_plugins() {
    let registry = get_global_registry();
    let mut guard = registry.lock().unwrap();
    guard.clear();
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPlugin;

    impl ParsanolPlugin for TestPlugin {
        fn name(&self) -> &str {
            "test"
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn description(&self) -> &str {
            "A test plugin"
        }
    }

    #[test]
    fn test_plugin_registry() {
        let mut registry = PluginRegistry::new();
        assert!(registry.is_empty());

        let result = registry.register_plugin(Box::new(TestPlugin));
        assert!(result);
        assert_eq!(registry.len(), 1);

        let info = registry.get_plugin_info("test").unwrap();
        assert_eq!(info.name, "test");
        assert_eq!(info.version, "1.0.0");

        // Can't register same plugin twice
        let result = registry.register_plugin(Box::new(TestPlugin));
        assert!(!result);

        // Unregister
        let result = registry.unregister_plugin("test");
        assert!(result);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_atom_registry() {
        use super::super::custom::{CustomAtom, CustomResult};

        struct TestAtom;
        impl CustomAtom for TestAtom {
            fn parse(&self, _input: &str, _pos: usize) -> Option<CustomResult> {
                None
            }
            fn description(&self) -> &str {
                "test atom"
            }
        }

        let mut registry = AtomRegistry::new();
        assert!(registry.is_empty());

        let result = registry.register("test_atom", Box::new(TestAtom), "test_plugin");
        assert!(result);
        assert_eq!(registry.len(), 1);

        let entry = registry.get("test_atom").unwrap();
        assert_eq!(entry.plugin, "test_plugin");
        assert_eq!(entry.description, "test atom");

        // Can't register same atom twice
        let result = registry.register("test_atom", Box::new(TestAtom), "other");
        assert!(!result);

        // Unregister
        let result = registry.unregister("test_atom");
        assert!(result);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_transform_registry() {
        let mut registry = TransformRegistry::new();
        assert!(registry.is_empty());

        let result = registry.register(
            "test_transform",
            "test_plugin",
            "A test transform",
            vec!["number".to_string(), "string".to_string()],
        );
        assert!(result);
        assert_eq!(registry.len(), 1);

        let entry = registry.get("test_transform").unwrap();
        assert_eq!(entry.plugin, "test_plugin");
        assert_eq!(entry.description, "A test transform");
        assert_eq!(entry.patterns.len(), 2);

        // Can't register same transform twice
        let result = registry.register("test_transform", "other", "other", vec![]);
        assert!(!result);

        // Unregister
        let result = registry.unregister("test_transform");
        assert!(result);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_global_registry() {
        // Clear any existing plugins
        clear_plugins();

        assert_eq!(plugin_count(), 0);

        let result = register_plugin(Box::new(TestPlugin));
        assert!(result);
        assert_eq!(plugin_count(), 1);

        let info = get_plugin_info("test").unwrap();
        assert_eq!(info.name, "test");

        let plugins = list_plugins();
        assert_eq!(plugins.len(), 1);

        // Cleanup
        clear_plugins();
        assert_eq!(plugin_count(), 0);
    }
}
