use std::slice;

use quote::ToTokens;
use syn::{
    punctuated::{self},
    token::PathSep,
    visit_mut::{self, VisitMut},
    Ident, ItemUse, PathSegment, Result, UseTree,
};

/// Implements [`syn::visit_mut::VisitMut`] to find
/// those crate [`Path`](syn::Path)s which match
/// [`target`](Self::target) and replace them with [`replacement`](Self::replacement).
pub(crate) struct BpafPathReplacer {
    query: SimplePath,
    replacement: syn::Path,
}

fn check_simple(path: syn::Path) -> Result<syn::Path> {
    if path.iter().all(|seg| seg.arguments.is_none()) {
        Ok(path)
    } else {
        Err(syn::Error::new_spanned(
            path,
            format_args!(
                "bpaf crate path should not contain path arguments (items in angle brackets <..>)."
            ),
        ))
    }
}

impl BpafPathReplacer {
    pub(crate) fn new(query: syn::Path, replacement: syn::Path) -> Result<Self> {
        Ok(BpafPathReplacer {
            query: check_simple(query).map(|query| SimplePath {
                leading_colon: query.leading_colon,
                segments: query.segments.into_iter().map(|s| s.ident).collect(),
            })?,
            replacement: check_simple(replacement)?,
        })
    }

    /// First checks if both [`query`](Self::query) and `other` have the
    /// leading path segment (`::`, which marks [a path as
    /// global](https://doc.rust-lang.org/reference/procedural-macros.html#procedural-macro-hygiene))
    /// and the same [`Ident`]s forming the prefix of the path. If there is a
    /// match, the prefix of `target` will be replacement with [`replacement`](Self::replacement)
    fn replace_if_match<'a, P: InputPath>(&self, other: &'a P) -> Option<P> {
        let prefix_matcher = PrefixMatcher::new(&self.query, other);
        prefix_matcher
            .get_suffix()
            .map(|suffix| P::concatenate(self.replacement.clone(), suffix).unwrap())
    }
}

pub struct SimplePath {
    leading_colon: Option<PathSep>,
    segments: Vec<Ident>,
}

pub trait PathPart: Clone {
    fn ident(&self) -> &Ident;
    fn from_ident(id: Ident) -> Self;
}

impl PathPart for Ident {
    fn ident(&self) -> &Ident {
        &self
    }

    fn from_ident(id: Ident) -> Self {
        id
    }
}

pub trait CratePath: Sized {
    type Part: PathPart;
    type PartIter<'a>: Iterator<Item = &'a Self::Part> + CloneRemainder
    where
        Self: 'a;

    fn leading_colon(&self) -> Option<PathSep>;

    fn iter(&self) -> Self::PartIter<'_>;
}

pub trait CloneRemainder {}

struct MatchRemainder<P: CratePath> {
    replaced_path: P,
    remainder: Option<P>,
}

pub struct PrefixMatcher<'a, P: CratePath> {
    query: &'a SimplePath,
    target: &'a P,
}

impl<'a, P: CratePath> PrefixMatcher<'a, P> {
    fn new(query: &'a SimplePath, target: &'a P) -> PrefixMatcher<'a, P> {
        Self { query, target }
    }

    /// Get the tail part of [`target`](Self::target), if its prefix to match
    /// [`query`](Self::query). If there is no prefix match, then return None;
    fn get_suffix(&self) -> Option<P::PartIter<'a>> {
        if self.query.leading_colon() == self.target.leading_colon() {
            BaseMatchIter {
                query_iter: self.query.iter(),
                target_iter: self.target.iter(),
                status: Option::<MatchStatus<P>>::None,
            }
            .last()
            .and_then(|status| match status {
                MatchStatus::Complete { tail } => Some(tail),
                _ => None,
            })
        } else {
            None
        }
    }
}

pub struct BaseMatchIter<'a, P: CratePath> {
    query_iter: slice::Iter<'a, Ident>,
    target_iter: P::PartIter<'a>,
    status: Option<MatchStatus<'a, P>>,
}

impl<'a, P: CratePath> Iterator for BaseMatchIter<'a, P> {
    type Item = MatchStatus<'a, P>;

    fn next(&mut self) -> Option<Self::Item> {
        self.status
            .update(self.query_iter.next(), self.target_iter.next());
        self.status
    }
}

impl CratePath for SimplePath {
    type Part = Ident;
    type PartIter<'a> = slice::Iter<'a, Ident>;

    fn leading_colon(&self) -> Option<PathSep> {
        self.leading_colon
    }

    fn iter(&self) -> Self::PartIter<'_> {
        self.segments.iter()
    }
}

trait InputPath: CratePath {
    fn concatenate(prefix: syn::Path, suffix: Self::PartIter<'_>) -> Result<Self>;
}

pub trait PathPartIter<'a, X: PathPart + 'a>: Iterator<Item = &'a X> {
    fn collect<P: CratePath<Part = X>>(&self) -> P;
}

pub(crate) trait CollectIntoPath<P: CratePath> {
    fn collect_into_path(&self) -> P;
}

impl<X, P, Y> CollectIntoPath<P> for Y
where
    P: CratePath<Part = X>,
    Y: Iterator<Item = X>,
{
    fn collect_into_path(&self) -> P {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct MatchStatus<'a, P: CratePath + 'a> {
    mismatch: bool,
    match_complete: bool,
    matched_prefix: Vec<&'a P::Part>,
    suffix: Option<P>,
}

impl<'a, P: CratePath + 'a> MatchStatus<'a, P> {
    fn update(
        &mut self,
        query_iter: slice::Iter<'_, Ident>,
        target_iter: &'a P::PartIter<'a>,
    ) -> bool {
        if self.mismatch {
            false
        } else {
            match (query_iter.next(), target_iter.next()) {
                (Some(q), Some(t)) => {
                    if q == t.ident() {
                        self.matched_prefix.push(t);
                    } else {
                        self.mismatch = true;
                    }
                }
                (None, Some(t)) => {
                    self.match_complete = true;
                    self.suffix = t.clone_remainder();
                }
                (_, None) => false,
            };

            !self.mismatch
        }
    }
}

impl PathPart for PathSegment {
    fn ident(&self) -> &Ident {
        let Self { ident, arguments } = self;
        if arguments.is_none() {
            ident
        } else {
            panic!("Crate paths cannot contain angle brackets")
        }
    }

    fn from_ident(ident: Ident) -> Self {
        PathSegment {
            ident,
            arguments: Default::default(),
        }
    }
}

impl CratePath for syn::Path {
    type Part = PathSegment;
    type PartIter<'a> = punctuated::Iter<'a, PathSegment>;

    fn leading_colon(&self) -> Option<PathSep> {
        self.leading_colon
    }

    fn iter(&self) -> Self::PartIter<'_> {
        self.segments.iter()
    }
}

impl InputPath for syn::Path {
    fn concatenate(_: &Self, prefix: syn::Path, suffix: Self::PartIter<'_>) -> Result<Self> {
        Ok(Self {
            leading_colon: prefix.leading_colon,
            segments: prefix.segments.into_iter().chain(suffix.cloned()).collect(),
        })
    }
}
impl PathPart for UseTree {
    fn ident(&self) -> &Ident {
        todo!()
    }

    fn from_ident(id: Ident) -> Self {
        todo!()
    }
}

#[derive(Clone)]
pub struct TreeIter<'a> {
    next_tree: Option<&'a UseTree>,
    rest_of: Option<UseTree>,
}

impl<'a> Iterator for TreeIter<'a>
where
    Self: 'a,
{
    type Item = &'a UseTree;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(tree) = self.next_tree {
            self.next_tree = match tree {
                UseTree::Path(use_path) => use_path.tree.as_ref().into(),
                UseTree::Name(_) | UseTree::Rename(_) | UseTree::Glob(_) => None,
                // TODO: when would the global bpaf path be a part of a tree?
                // Likely just better to insist that macro writers always `use`
                // bpaf without using a tree structure
                UseTree::Group(_) => None,
            };
            Some(tree)
        } else {
            None
        }
    }
}

impl CratePath for ItemUse {
    type Part = UseTree;

    type PartIter<'a> = TreeIter<'a>;

    fn leading_colon(&self) -> Option<PathSep> {
        todo!()
    }

    fn iter(&self) -> Self::PartIter<'_> {
        todo!()
    }
}

fn concat_prefix_path_to_tree(
    original_prefix: &UseTree,
    prefix: syn::Path,
    suffix: &UseTree,
) -> UseTree {
    match original_prefix {
        UseTree::Path(_) => todo!(),
        UseTree::Name(_) => todo!(),
        UseTree::Rename(_) => todo!(),
        UseTree::Glob(_) => todo!(),
        UseTree::Group(_) => todo!(),
    }
}

impl InputPath for ItemUse {
    fn concatenate(
        original_prefix: &Self,
        prefix: syn::Path,
        suffix: Self::PartIter<'_>,
    ) -> Result<Self> {
        Ok(Self {
            attrs: original_prefix.attrs,
            vis: original_prefix.vis,
            use_token: original_prefix.use_token,
            leading_colon: prefix.leading_colon,
            tree: {
                concat_prefix_path_to_tree(
                    original_prefix.iter().last().ok_or_else(|| {
                        syn::Error::new_spanned(
                            original_prefix,
                            format_args!("Expecting a non-empty path to replace."),
                        )
                    })?,
                    prefix,
                    suffix,
                )
            },
            semi_token: original_prefix.semi_token,
        })
    }
}

impl VisitMut for BpafPathReplacer {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        if let Some(replaced) = self.replace_if_match(path) {
            *path = replaced;
        }
        visit_mut::visit_path_mut(self, path);
    }

    fn visit_item_use_mut(&mut self, item_use: &mut ItemUse) {
        if let Some(replaced) = self.replace_if_match(item_use) {
            *item_use = replaced;
        }
        visit_mut::visit_item_use_mut(self, item_use);
    }
}
