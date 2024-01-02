use syn::{punctuated::Punctuated, visit_mut::VisitMut};

use crate::attrs::PostDecor;

/// Checks if a custom `bpaf_path` has been specified and returns it. If one
/// was not specified, then return `::bpaf` (the absolute path), by default.
///
/// Note: if `bpaf_path` is defined multiple times, the last definition
/// overrides all others.
pub(crate) fn extract_bpaf_path(decors: &[PostDecor]) -> Option<syn::Path> {
    decors.iter().rev().find_map(|a| match a {
        PostDecor::CratePath { bpaf_path, .. } => Some(bpaf_path.clone()),
        _ => None,
    })
}

/// Implements [`syn::visit_mut::VisitMut`] to find
/// those [`Path`](syn::Path)s which match
/// [`target`](Self::target) and replace them with [`replacement`](Self::replacement).
pub(crate) struct PathPrefixReplacer {
    target: syn::Path,
    replacement: syn::Path,
}

impl PathPrefixReplacer {
    pub(crate) fn new(target: syn::Path, replacement: syn::Path) -> Self {
        PathPrefixReplacer {
            target,
            replacement,
        }
    }

    /// Check if both [`target`](Self::target) and `other` have the same kind of
    /// leading path segment (`::`), which marks [a path as
    /// global](https://doc.rust-lang.org/reference/procedural-macros.html#procedural-macro-hygiene).
    ///
    /// If these do not match, no replacement will be performed.
    fn matches_target_global(&self, other: &mut syn::Path) -> bool {
        self.target.leading_colon.is_some() && other.leading_colon.is_some()
    }

    /// Check if the initial segments of `other` match [`target`](Self::target).
    ///
    /// If these do not match, no replacement will be performed.
    fn matches_target_segments(&self, other: &mut syn::Path) -> bool {
        self.target
            .segments
            .iter()
            .zip(other.segments.iter())
            .all(|(f, o)| f == o)
    }

    /// Replaces the prefix of `other` with those of [`replacement`](Self::replacement).
    fn replace_if_matches(&self, other: &mut syn::Path) {
        if self.matches_target_global(other) && self.matches_target_segments(other) {
            other.leading_colon = self.replacement.leading_colon;
            other.segments = self
                .replacement
                .segments
                .clone()
                .into_iter()
                .chain(
                    other
                        .segments
                        .iter()
                        .skip(self.target.segments.iter().count())
                        .cloned(),
                )
                .collect::<Punctuated<_, _>>();
        }
    }
}

impl VisitMut for PathPrefixReplacer {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        self.replace_if_matches(path)
    }
}
