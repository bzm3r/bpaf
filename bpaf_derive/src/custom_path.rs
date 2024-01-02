use syn::{
    punctuated::{self, Punctuated},
    token::PathSep,
    visit_mut::{self, VisitMut},
    Ident, Item, ItemUse, PathArguments, PathSegment, UseName, UsePath, UseRename, UseTree,
};

/// Implements [`syn::visit_mut::VisitMut`] to find
/// those crate [`Path`](syn::Path)s which match
/// [`target`](Self::target) and replace them with [`replacement`](Self::replacement).
pub(crate) struct BpafPathReplacer {
    target: syn::Path,
    replacement: syn::Path,
}

/// There are two kinds of "paths" we must deal with:[`syn::Path`] and
/// [`syn::UseTree`] (which comes up when parsing a [`syn::ItemUse`]).
///
/// [`syn::Path`] has simple parts which are all [`Ident`]s, but a
/// [`syn::UseTree`] might have other kinds of parts (see its documentation for details).
pub enum PathPart<'a> {
    /// Part of a path-like which is essentially an [`Ident`].
    Ident(Box<&'a dyn IdentRef>),
    Other(&'a UseTree),
}

impl<'a> PartialEq for PathPart<'a> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Ident(l), Self::Ident(r)) => l == r,
            (Self::Other(l), Self::Other(r)) => l == r,
            _ => false,
        }
    }
}

/// Marker trait representing references (mutable or immutable) to [`Ident`]s.
pub(crate) trait IdentRef {
    fn ident(&self) -> &Ident;
}

impl IdentRef for &Ident {
    fn ident(&self) -> &Ident {
        &self
    }
}

impl IdentRef for &mut Ident {
    fn ident(&self) -> &Ident {
        &self
    }
}

impl IdentRef for &UsePath {
    fn ident(&self) -> &Ident {
        &self.ident
    }
}

impl IdentRef for &UseName {
    fn ident(&self) -> &Ident {
        &self.ident
    }
}

impl IdentRef for &UseRename {
    fn ident(&self) -> &Ident {
        &self.ident
    }
}

/// Marker trait representing references (mutable or immutable) to [`UseTree`]s.
pub(crate) trait TreeRef {}

impl TreeRef for &UseTree {}

impl TreeRef for &mut UseTree {}

impl<'a> From<Id> for PathPart<'a> {
    fn from(value: Box<&'a dyn IdentRef>) -> Self {
        PathPart::Ident(value)
    }
}

impl<'a, Id: IdentRef> From<&'a UseTree> for PathPart<'a, Id> {
    fn from(value: &'a UseTree) -> Self {
        PathPart::Other(value)
    }
}

impl<'a, Id: IdentRef> From<&'a mut UseTree> for PathPart<'a, Id> {
    fn from(value: &'a mut UseTree) -> Self {
        PathPart::Other(value)
    }
}

pub struct PathIter<'a> {
    segments: punctuated::Iter<'a, PathSegment>,
}

impl<'a> Iterator for PathIter<'a> {
    type Item = PathPart<'a, &'a Ident>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.segments.next() {
            if PathArguments::None == next.arguments {
                return Some((&next.ident).into());
            }
        }

        None
    }
}

pub struct PathIterMut<'a> {
    segments: punctuated::IterMut<'a, PathSegment>,
}

impl<'a> Iterator for PathIterMut<'a> {
    type Item = PathPart<'a, &'a mut Ident>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.segments.next() {
            if PathArguments::None == next.arguments {
                return Some((&mut next.ident).into());
            }
        }

        None
    }
}

pub struct UsePathIter<'a> {
    tree: Option<&'a UseTree>,
}

impl<'a> Iterator for UsePathIter<'a> {
    type Item = PathPart<'a, &'a Ident>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(tree) = self.tree.take() {
            return match tree {
                UseTree::Path(UsePath {
                    ident, tree: next, ..
                }) => {
                    self.tree.replace(next);
                    Some(ident.into())
                }
                UseTree::Name(UseName { ident }) => Some(ident.into()),
                UseTree::Rename(UseRename { ident, .. }) => Some(ident.into()),
                tree @ (UseTree::Glob(_) | UseTree::Group(_)) => Some(tree.into()),
            };
        }

        None
    }
}

pub struct UsePathIterMut<'a> {
    tree: Option<&'a mut UseTree>,
}

impl<'a> Iterator for UsePathIterMut<'a> {
    type Item = PathPart<'a, &'a mut Ident>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(tree) = self.tree.take() {
            return match tree {
                UseTree::Path(UsePath {
                    ident, tree: next, ..
                }) => {
                    self.tree.replace(next);
                    Some(ident.into())
                }
                UseTree::Name(UseName { ident }) => Some(ident.into()),
                UseTree::Rename(UseRename { ident, .. }) => Some(ident.into()),
                tree @ (UseTree::Glob(_) | UseTree::Group(_)) => Some(tree.into()),
            };
        }

        None
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PathType {
    Path,
    UseItem,
}

pub(crate) trait RustPath: Sized {
    type Iter<'a>: Iterator<Item = PathPart<'a, &'a Ident>>
    where
        Self: 'a;
    type IterMut<'a>: Iterator<Item = PathPart<'a, &'a mut Ident>>
    where
        Self: 'a;

    fn leading_colon(&self) -> Option<PathSep>;
    fn leading_colon_mut(&mut self) -> &mut Option<PathSep>;
    fn iter_path(&self) -> Self::Iter<'_>;
    fn iter_path_mut(&mut self) -> Self::IterMut<'_>;
    fn from_path_parts<'b, I: Iterator<Item = PathPart<'b, &'b Ident>>>(
        &self,
        leading_colon: Option<PathSep>,
        last_match: Option<UseTree>,
        parts: I,
    ) -> Self;
    fn replace_prefix(&self, prefix_count: usize, replace_with: &syn::Path) -> Self {
        let mut this_parts = self.iter_path().skip(prefix_count - 1);
        let last_match = this_parts
            .next()
            .expect("At least one match must have occurred.");
        self.from_path_parts(
            replace_with.leading_colon(),
            replace_with
                .iter_path()
                .chain(self.iter_path().skip(prefix_count)),
        )
    }
}

impl RustPath for syn::Path {
    type Iter<'a> = PathIter<'a>;

    type IterMut<'a> = PathIterMut<'a>;

    fn leading_colon(&self) -> Option<PathSep> {
        self.leading_colon
    }

    fn leading_colon_mut(&mut self) -> &mut Option<PathSep> {
        &mut self.leading_colon
    }

    fn iter_path(&self) -> Self::Iter<'_> {
        PathIter {
            segments: self.segments.iter(),
        }
    }

    fn iter_path_mut(&mut self) -> Self::IterMut<'_> {
        PathIterMut {
            segments: self.segments.iter_mut(),
        }
    }

    fn from_path_parts<'b, I: Iterator<Item = PathPart<'b, &'b Ident>>>(
        &self,
        leading_colon: Option<PathSep>,
        parts: I,
    ) -> Self {
        syn::Path {
            leading_colon,
            segments: parts
                .map(|p| match p {
                    PathPart::Ident(id) => PathSegment {
                        arguments: PathArguments::None,
                        ident: id.clone(),
                    },
                    PathPart::Other(o) => panic!("PathPart is {o:?} when an Ident was expected."),
                })
                .collect::<Punctuated<_, _>>(),
        }
    }
}

impl RustPath for ItemUse {
    type Iter<'a> = UsePathIter<'a>;

    type IterMut<'a> = UsePathIterMut<'a>;

    fn leading_colon(&self) -> Option<PathSep> {
        self.leading_colon
    }

    fn leading_colon_mut(&mut self) -> &mut Option<PathSep> {
        &mut self.leading_colon
    }

    fn iter_path(&self) -> Self::Iter<'_> {
        UsePathIter {
            tree: Some(&self.tree),
        }
    }

    fn iter_path_mut(&mut self) -> Self::IterMut<'_> {
        UsePathIterMut {
            tree: Some(&mut self.tree),
        }
    }

    fn from_path_parts<'b, I: Iterator<Item = PathPart<'b, &'b Ident>>>(
        &self,
        leading_colon: Option<PathSep>,
        parts: I,
    ) -> Self {
        let Self {
            attrs,
            vis,
            use_token,
            semi_token,
            ..
        } = self;

        Self {
            attrs,
            vis,
            use_token,
            leading_colon,
            tree: parts.map(|p| match p {
                PathPart::Ident(_) => todo!(),
                PathPart::Other(_) => todo!(),
            }),
            semi_token,
        }
    }
}

impl BpafPathReplacer {
    pub(crate) fn new(target: syn::Path, replacement: syn::Path) -> Self {
        BpafPathReplacer {
            target,
            replacement,
        }
    }

    /// Check if both [`target`](Self::target) and `other` have the same kind of
    /// leading path segment (`::`), which marks [a path as
    /// global](https://doc.rust-lang.org/reference/procedural-macros.html#procedural-macro-hygiene).
    ///
    /// If these do not match, no replacement will be performed.
    fn matches_target_leading_colon<P: RustPath>(&self, other: &P) -> bool {
        self.target.leading_colon().is_some() && other.leading_colon().is_some()
    }

    /// Check if the initial segments of `other` match [`target`](Self::target).
    ///
    /// If these do not match, no replacement will be performed.
    fn matches_target_segments<P: RustPath>(&self, other: &P) -> bool {
        self.target
            .iter_path()
            .zip(other.iter_path())
            .all(|(f, o)| f == o)
    }

    /// Replaces the prefix of `other` with those of [`replacement`](Self::replacement).
    fn replace_prefix_if_match<P: RustPath>(&self, other: &mut P) {
        if self.matches_target_leading_colon(other) && self.matches_target_segments(other) {
            *other = other.replace_prefix(&self.replacement);
        }
    }
}

impl VisitMut for BpafPathReplacer {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        self.replace_prefix_if_match(path);
        visit_mut::visit_path_mut(self, path);
    }

    fn visit_item_use_mut(&mut self, item_use: &mut ItemUse) {
        visit_mut::visit_item_use_mut(self, item_use);
    }
}
