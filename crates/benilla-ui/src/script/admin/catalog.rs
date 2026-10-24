//! Bounded, cached catalog search for the fork Admin layer. Only visible rows cross into Lua.

use std::collections::HashSet;

use super::{AdminAction, AdminCatalogEntry};

pub const PAGE_SIZE: usize = 5;
pub const MAX_CATALOG_ENTRIES: usize = 100_000;
pub const MAX_LABEL_BYTES: usize = 512;
pub const MAX_QUERY_CHARS: usize = 80;

struct SearchEntry {
    source_index: usize,
    label: String,
    id: String,
}

pub(super) struct CatalogSearch {
    entries: Vec<SearchEntry>,
    query: Option<String>,
    matches: Vec<usize>,
    pub error: Option<&'static str>,
    #[cfg(test)]
    pub scanned: usize,
}

impl CatalogSearch {
    pub fn new(action: AdminAction, catalog: &[AdminCatalogEntry]) -> Self {
        let mut search = Self {
            entries: Vec::new(),
            query: None,
            matches: Vec::new(),
            error: None,
            #[cfg(test)]
            scanned: 0,
        };
        if catalog.len() > MAX_CATALOG_ENTRIES {
            search.error = Some("Catalog exceeds the supported browse limit. Ask the world host to supply a bounded catalog.");
            return search;
        }
        let mut ids = HashSet::with_capacity(catalog.len());
        for entry in catalog {
            if entry.label.len() > MAX_LABEL_BYTES
                || entry.label.trim().is_empty()
                || entry.label.chars().any(char::is_control)
                || (entry.id == 0 && action != AdminAction::Weather)
                || !ids.insert(entry.id)
            {
                search.error = Some("Catalog contains an invalid or ambiguous entry. Ask the world host to refresh it.");
                return search;
            }
        }
        search.entries = catalog
            .iter()
            .enumerate()
            .map(|(source_index, entry)| SearchEntry {
                source_index,
                label: entry.label.to_lowercase(),
                id: entry.id.to_string(),
            })
            .collect();
        search.entries.sort_unstable_by(|a, b| {
            a.label
                .cmp(&b.label)
                .then_with(|| catalog[a.source_index].id.cmp(&catalog[b.source_index].id))
        });
        search
    }

    /// A query scans at most MAX_CATALOG_ENTRIES pre-folded, bounded names once. Unchanged
    /// refreshes and page changes reuse the result indices; they never rescan the catalog.
    pub fn filter(&mut self, query: &str) -> Result<(), &'static str> {
        if query.len() > MAX_QUERY_CHARS * 4 || query.chars().count() > MAX_QUERY_CHARS {
            return Err("Search is limited to 80 characters.");
        }
        if query.chars().any(char::is_control) {
            return Err("Search cannot contain control characters.");
        }
        if let Some(error) = self.error {
            return Err(error);
        }
        let query = query.trim().to_lowercase();
        if self.query.as_ref() == Some(&query) {
            return Ok(());
        }
        self.matches.clear();
        // Exact ID, then name/ID prefixes, then name substrings; alphabetical/ID order
        // within each rank. Empty queries preserve that deterministic browse order.
        let mut prefix = Vec::new();
        let mut substring = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            #[cfg(test)]
            {
                self.scanned += 1;
            }
            if !query.is_empty() && entry.id == query {
                self.matches.push(index);
            } else if entry.label.starts_with(&query) || entry.id.starts_with(&query) {
                prefix.push(index);
            } else if entry.label.contains(&query) {
                substring.push(index);
            }
        }
        self.matches.extend(prefix);
        self.matches.extend(substring);
        self.query = Some(query);
        Ok(())
    }

    pub fn total(&self) -> usize {
        self.matches.len()
    }

    pub fn pages(&self) -> usize {
        self.total().div_ceil(PAGE_SIZE)
    }

    pub fn page(&self, requested: i64) -> usize {
        (requested.max(1) as u64).min(self.pages().max(1) as u64) as usize
    }

    pub fn page_indices(&self, page: usize) -> impl Iterator<Item = usize> + '_ {
        self.matches
            .iter()
            .skip((page - 1) * PAGE_SIZE)
            .take(PAGE_SIZE)
            .map(|index| self.entries[*index].source_index)
    }
}
