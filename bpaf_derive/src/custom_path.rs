use std::{
    clone,
    iter::{Cloned, FromIterator, Map, Peekable},
    slice,
};

use syn::{
    punctuated::{self, Punctuated},
    token::{Match, PathSep},
    visit_mut::{self, VisitMut},
    Ident, Item, ItemUse, PathArguments, PathSegment, UseName, UsePath, UseRename, UseTree,
};

/// Implements [`syn::visit_mut::VisitMut`] to find
/// those crate [`Path`](syn::Path)s which match
/// [`target`](Self::target) and replace them with [`replacement`](Self::replacement).
pub(crate) struct BpafPathReplacer {
    query: syn::Path,
    replacement: syn::Path,
}

impl BpafPathReplacer {
    pub(crate) fn new(query: syn::Path, replacement: syn::Path) -> Self {
        BpafPathReplacer { query, replacement }
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

pub struct Query {
    leading_colon: Option<PathSep>,
    segments: Vec<Ident>,
}

pub struct Replacement {
    leading_colon: Option<PathSep>,
    segments: Vec<Ident>,
}

impl Replacement {
    fn path_parts_iter<X: PathPart>(&self) -> Map<Cloned<slice::Iter<'_, Ident>>, fn(Ident) -> X> {
        self.segments.iter().cloned().map(<X as From<Ident>>::from)
    }
}

pub struct Target<S: CratePath> {
    leading_colon: Option<PathSep>,
    segments: S,
}

pub trait PathPart: From<Ident> + Clone {
    fn ident(&self) -> &Ident;
}

pub trait CratePath: FromIterator<Self::Part> {
    type Part: PathPart;
    type Iter<'a>: Iterator<Item = &'a Self::Part> + Clone
    where
        Self: 'a;

    fn leading_colon(&self) -> Option<PathSep>;

    fn iter(&self) -> Self::Iter<'_>;
}

impl CratePath for syn::Path {
    type Part = PathSegment;

    type Iter<'a> = PathPartIter;

    fn leading_colon(&self) -> Option<PathSep> {
        todo!()
    }

    fn iter(&self) -> Self::Iter<'_> {
        todo!()
    }
}

pub enum MatchStatus<'a, P: CratePath + 'a> {
    Begin,
    Partial { current: &'a P::Part },
    Complete { tail: P::Iter<'a> },
    Different,
}

pub struct Matcher<'a, P: CratePath> {
    query: &'a Query,
    replacement: &'a Replacement,
    target: &'a Target<P>,
}

impl<'a, P: CratePath> Matcher<'a, P> {
    fn new(
        query: &'a Query,
        replacement: &'a Replacement,
        target: &'a Target<P>,
    ) -> Matcher<'a, P> {
        Self {
            query,
            replacement,
            target,
        }
    }

    fn match_iter(&self) -> MatchIter<'a, P> {
        if self.query.leading_colon == self.target.leading_colon {
            MatchIter::LeadingColonsMatch(BaseMatchIter {
                query_iter: self.query.segments.iter().peekable(),
                target_iter: self.target.segments.iter(),
                status: MatchStatus::Begin,
            })
        } else {
            MatchIter::LeadingColonsMismatch
        }
    }

    fn maybe_replace(&self) -> Option<P> {
        self.match_iter().last().and_then(|status| match status {
            MatchStatus::Begin => unreachable!(
                "If MatchIter has been consumed, it should not return MatchStatus::Begin."
            ),
            MatchStatus::Partial { current } => unreachable!(
                "If MatchIter has been consumed, it should not return MatchStatus::Partial."
            ),
            MatchStatus::Complete { tail } => Some(
                self.replacement
                    .path_parts_iter::<P::Part>()
                    .chain(tail.map(<P::Part>::clone))
                    .collect(),
            ),
            MatchStatus::Different => None,
        })
    }
}

pub enum MatchIter<'a, P: CratePath + 'a> {
    /// Leading colons matched.
    LeadingColonsMatch(BaseMatchIter<'a, P>),
    /// Leading colons did not match.
    LeadingColonsMismatch,
}

impl<'a, P: CratePath + 'a> MatchIter<'a, P> {
    fn status(&self) -> Option<MatchStatus<'a, P>> {
        match self {
            MatchIter::LeadingColonsMatch(match_iter) => Some(match_iter.status),
            MatchIter::LeadingColonsMismatch => None,
        }
    }

    fn concat(&self, replacement: &'a Replacement) {
        match self.status() {
            Some(m) => todo!(),
            None => todo!(),
        }
    }
}

pub struct BaseMatchIter<'a, P: CratePath + 'a> {
    query_iter: Peekable<slice::Iter<'a, Ident>>,
    target_iter: P::Iter<'a>,
    status: MatchStatus<'a, P>,
}

impl<'a, P: CratePath> BaseMatchIter<'a, P> {
    fn match_parts(
        &mut self,
        query_part: &'a Ident,
        target_part: &'a P::Part,
    ) -> MatchStatus<'a, P> {
        self.status = match (self.query_iter.next(), self.target_iter.next()) {
            (Some(id), Some(part)) => {
                if id == part.ident() {
                    if let Some(_) = self.query_iter.peek() {
                        MatchStatus::Partial { current: part }
                    } else {
                        MatchStatus::Complete {
                            tail: self.target_iter.clone(),
                        }
                    }
                } else {
                    MatchStatus::Different
                }
            }
            (None, _) => match self.status {
                MatchStatus::Begin => MatchStatus::Different,
                MatchStatus::Partial { current } => MatchStatus::Different,
                status @ (MatchStatus::Complete { .. } | MatchStatus::Different) => status,
            },
            (Some(_), None) => MatchStatus::Different,
        };

        self.status
    }
}

impl<'a, P: CratePath> Iterator for MatchIter<'a, P> {
    type Item = MatchStatus<'a, P>;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

impl VisitMut for BpafPathReplacer {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        if let Some(replaced) = self.replace_if_match(path) {
            path = replaced;
        }
        visit_mut::visit_path_mut(self, path);
    }

    fn visit_item_use_mut(&mut self, item_use: &mut ItemUse) {
        if let Some(replaced) = self.replace_if_match(item_use) {
            item_use = replaced;
        }
        visit_mut::visit_item_use_mut(self, item_use);
    }
}
