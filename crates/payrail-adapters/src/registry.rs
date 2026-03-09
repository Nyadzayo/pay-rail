use std::collections::HashMap;

use crate::traits::PaymentAdapter;

/// Routes payment operations to the correct adapter based on provider name.
///
/// Supports concurrent operation of multiple adapters in a single deployment.
pub struct AdapterRegistry {
    adapters: HashMap<String, Box<dyn PaymentAdapter>>,
}

impl AdapterRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    /// Registers an adapter for the given provider name.
    ///
    /// Overwrites any previously registered adapter for the same provider.
    pub fn register(&mut self, provider: &str, adapter: Box<dyn PaymentAdapter>) {
        self.adapters.insert(provider.to_owned(), adapter);
    }

    /// Retrieves the adapter registered for the given provider name.
    pub fn get(&self, provider: &str) -> Option<&dyn PaymentAdapter> {
        self.adapters.get(provider).map(|a| a.as_ref())
    }

    /// Lists all registered provider names.
    pub fn providers(&self) -> Vec<&str> {
        self.adapters.keys().map(|k| k.as_str()).collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}
