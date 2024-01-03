use std::{
    convert::{TryFrom, TryInto},
    iter::Peekable,
    slice,
};

use quote::ToTokens;
use syn::{
    punctuated::{self},
    token::{Match, PathSep},
    visit_mut::{self, VisitMut},
    Ident, ItemUse, PathSegment, Result, UseTree,
};

/// Implements [`syn::visit_mut::VisitMut`] to find
/// those crate [`Path`](syn::Path)s which match
/// [`target`](Self::target) and replace them with [`replacement`](Self::replacement).
pub(crate) struct BpafPathReplacer {
    query: SimplePath,
    replacement: SimplePath,
}

impl BpafPathReplacer {
    pub(crate) fn new(query: syn::Path, replacement: syn::Path) -> Result<Self> {
        Ok(BpafPathReplacer {
            query: query.try_into()?,
            replacement: replacement.try_into()?,
        })
    }

    /// First checks if both [`query`](Self::query) and `other` have the
    /// leading path segment (`::`, which marks [a path as
    /// global](https://doc.rust-lang.org/reference/procedural-macros.html#procedural-macro-hygiene))
    /// and the same [`Ident`]s forming the prefix of the path. If there is a
    /// match, the prefix of `target` will be replacement with [`replacement`](Self::replacement)
    fn replace_if_match<'a, P: CratePath>(&self, other: &'a P) -> Option<P> {
        todo!()
    }
}

pub struct SimplePath {
    leading_colon: Option<PathSep>,
    segments: Vec<Ident>,
}

impl TryFrom<syn::Path> for SimplePath {
    type Error = syn::Error;

    fn try_from(path: syn::Path) -> Result<Self> {
        Ok(SimplePath {
            leading_colon: path.leading_colon,
            segments: TryFromIterator::try_from_iter(path, path.segments)?,
        })
    }
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
    type PartIter<'a>: Iterator<Item = &'a Self::Part> + Clone
    where
        Self: 'a;

    fn leading_colon(&self) -> Option<PathSep>;

    fn iter(&self) -> Self::PartIter<'_>;
}

struct MatchRemainder<> {
    replaced_path: P,
    remainder: Option<P>
}

pub trait InputMatcher: CratePath {
    type MatchRemainder;

    fn new_matcher(&self, query_iter: slice::Iter<Ident>, replacement: &SimplePath) -> Self;

    fn query_iter(&self) -> slice::Iter<'_, Ident>;

    fn input_iter(&self) -> <Self as CratePath>::PartIter<'_>;

    fn status(&self) -> MatchStatus<'_, Self>;

    fn status_mut(&self) -> MatchStatus<'_, Self>;

    fn next_match(&self) -> Option<MatchStatus<'_, Self>> {
        self.status = match (self.query_iter.next(), self.target_iter.next()) {
            (Some(query_part), Some(target_part)) => {
                if query_part == target_part.ident() {
                    Some(MatchStatus::Partial {
                        current: target_part,
                    })
                } else {
                    None
                }
            }
            (None, Some(target)) => match self.status {
                Some(MatchStatus::Partial { current }) => Some(MatchStatus::Complete {
                    tail: self.target_iter.clone(),
                }),
                Some(MatchStatus::Complete { .. }) => self.status,
                None => todo!(),
            },
            (Some(_query), None) => None,
            (None, None) => None,
        };

        self.status
    }

    fn matching_iter(
        &self,
        query_iter: slice::Iter<&'_ Ident>,
    ) -> Option<Self::PartMatchingIter<'_>>;
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

pub trait TryFromIterator<A>: Sized {
    fn try_from_iter<Ctx: ToTokens, T: IntoIterator<Item = A>>(
        context_tokens: Ctx,
        iter: T,
    ) -> Result<Self>;
}

impl TryFromIterator<PathSegment> for Vec<Ident> {
    fn try_from_iter<Ctx: ToTokens, T: IntoIterator<Item = PathSegment>>(
        context_tokens: Ctx,
        iter: T,
    ) -> Result<Self> {
        iter.into_iter().map(|s| {
            let PathSegment { ident, arguments } = s;
            if arguments.is_none() {
                Ok(ident.clone())
            } else {
                Err(syn::Error::new_spanned(context_tokens, format_args!("bpaf crate path should not contain path arguments (items in angle brackets <..>).")))
            }
        })
        .collect::<Result<_>>()
    }
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

#[derive(Clone, Copy, Debug)]
pub enum MatchStatus<'a, P: InputMatcher + 'a> {
    Partial { current: &'a P::Part },
    Complete { tail: P::MatchRemainder },
}

pub struct Matcher<'a, P: InputMatcher> {
    query: &'a SimplePath,
    replacement: &'a SimplePath,
    target: &'a P,
}

impl<'a, P: InputMatcher> Matcher<'a, P> {
    fn new(query: &'a SimplePath, replacement: &'a SimplePath, target: &'a P) -> Matcher<'a, P> {
        Self {
            query,
            replacement,
            target,
        }
    }

    fn match_iter(&self) -> P::PartMatchingIter<'_> {
        if self.query.leading_colon() == self.target.leading_colon() {
            MatchIter::LeadingColonsMatch(self.target.matching_iter(self.query.iter()))
        } else {
            MatchIter::LeadingColonsMismatch
        }
    }

    fn maybe_replace(&self) -> Option<P> {
        self.match_iter().last().and_then(|status| match status {
            MatchStatus::Partial { .. } => None,
            MatchStatus::Complete { tail } => Some(todo!()),
        })
    }
}

// pub enum MatchIter<'a, P: CratePath + 'a> {
//     /// Leading colons matched.
//     LeadingColonsMatch(BaseMatchIter<'a, P>),
//     /// Leading colons did not match.
//     LeadingColonsMismatch,
// }

// impl<'a, P: CratePath + 'a> MatchIter<'a, P> {
//     fn status(&self) -> Option<MatchStatus<'a, P>> {
//         match self {
//             MatchIter::LeadingColonsMatch(match_iter) => match_iter.status,
//             MatchIter::LeadingColonsMismatch => None,
//         }
//     }
// }

impl<'a, P: CratePath> Iterator for MatchIter<'a, P> {
    type Item = MatchStatus<'a, P>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            MatchIter::LeadingColonsMatch(base_match_iter) => base_match_iter.next(),
            MatchIter::LeadingColonsMismatch => None,
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

#[derive(Clone)]
pub struct PathMatchingIter<'a> {
    query: slice::Iter<'a, Ident>,
    target: punctuated::Iter<'a, PathSegment>,
    status: MatchStatus<'a, syn::Path>,
}

impl<'a> Iterator for PathMatchingIter<'a>
where
    Self: 'a,
{
    type Item = &'a PathSegment;

    fn next(&mut self) -> Option<Self::Item> {
        self.status = match (self.query_iter.next(), self.target_iter.next()) {
            (Some(query_part), Some(target_part)) => {
                if query_part == target_part.ident() {
                    Some(MatchStatus::Partial {
                        current: target_part,
                    })
                } else {
                    None
                }
            }
            (None, Some(target)) => match self.status {
                Some(MatchStatus::Partial { current }) => Some(MatchStatus::Complete {
                    tail: self.target_iter.clone(),
                }),
                Some(MatchStatus::Complete { .. }) => self.status,
                None => todo!(),
            },
            (Some(_query), None) => None,
            (None, None) => None,
        };

        self.status
    }
}

impl CratePath for syn::Path {
    type Part = PathSegment;

    type PartMatchingIter<'a> = punctuated::Iter<'a, PathSegment>;

    fn leading_colon(&self) -> Option<PathSep> {
        self.leading_colon
    }

    fn iter(&self) -> Self::PartMatchingIter<'_> {
        self.segments.iter()
    }
}

impl TryFromIterator<Ident> for syn::Path {
    fn try_from_iter<Ctx: ToTokens, T: IntoIterator<Item = Ident>>(
        context_tokens: Ctx,
        iter: T,
    ) -> Result<Self> {
        todo!()
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
                UseTree::Group(use_group) => todo!(),
            };
            Some(tree)
        } else {
            None
        }
    }
}

impl CratePath for ItemUse {
    type Part = UseTree;

    type PartMatchingIter<'a> = TreeIter<'a>;

    fn leading_colon(&self) -> Option<PathSep> {
        todo!()
    }

    fn iter(&self) -> Self::PartMatchingIter<'_> {
        todo!()
    }
}

impl TryFromIterator<Ident> for ItemUse {
    fn try_from_iter<Ctx: ToTokens, T: IntoIterator<Item = Ident>>(
        context_tokens: Ctx,
        iter: T,
    ) -> Result<Self> {
        todo!()
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
