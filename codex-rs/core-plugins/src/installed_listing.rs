//! Bound installed-plugin listing to the identities the caller can return.
//! Full catalog listing keeps its existing metadata and discovery behavior.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum MarketplaceListing<'a> {
    All,
    InstalledAndSuggested {
        suggested_names: &'a HashSet<String>,
    },
}

impl PluginsManager {
    /// List installed plugins and explicit suggestions without hydrating unrelated
    /// catalog entries. Scope precedence, policy, and installed state are unchanged.
    pub fn list_installed_and_suggested_plugins_for_context(
        &self,
        context: &PluginMarketplaceContext,
        suggested_names: &HashSet<String>,
    ) -> Result<ConfiguredMarketplaceListOutcome, MarketplaceError> {
        context.list_marketplaces_selected(
            self,
            /*include_openai_curated*/ true,
            MarketplaceListing::InstalledAndSuggested { suggested_names },
        )
    }
}
