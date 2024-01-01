use std::iter::FromIterator;

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{
    punctuated::{Pair, Punctuated},
    visit_mut::VisitMut,
    Expr, Ident, PathSegment, Result,
};

use crate::{
    attrs::{EnumPrefix, PostDecor},
    field::StructField,
    top::{Body, Branch, EnumBranch, FieldSet},
};

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

pub struct FindAndReplaceVisitor {
    find: Ident,
    replace: Ident,
}

impl VisitMut for FindAndReplaceVisitor {
    fn visit_path_segment_mut(&mut self, seg: &mut syn::PathSegment) {
        let PathSegment { ident, .. } = seg;
        if ident == &self.find {
            *ident = self.replace.clone();
        }
    }
}

pub(crate) trait FindAndReplace: Sized {
    fn find_and_replace<T: Clone>(&self, find: &T, replace: &T) -> Result<Self>;
}

impl<X: FindAndReplace> FindAndReplace for Box<X> {
    fn find_and_replace<T: Clone>(&self, find: &T, replace: &T) -> Result<Self> {
        Ok(Box::new(self.as_ref().find_and_replace(find, replace)?))
    }
}

impl FindAndReplace for StructField {
    fn find_and_replace<T: Clone>(&self, find: &T, replace: &T) -> Result<Self> {
        let Self {
            name,
            env,
            naming,
            cons,
            postpr,
            help,
        } = self.clone();

        Ok(Self {
            name,
            env,
            naming,
            cons,
            postpr,
            help,
        })
    }
}

impl<X, S> FindAndReplace for Punctuated<X, S>
where
    S: Clone,
    X: Clone + FindAndReplace,
{
    fn find_and_replace<T: Clone>(&self, find: &T, replace: &T) -> Result<Self> {
        Ok(<Punctuated<X, S>>::from_iter(
            self.clone()
                .into_pairs()
                .map(|pair| match pair {
                    Pair::Punctuated(item, sep) => item
                        .find_and_replace(find, replace)
                        .map(|item| Pair::Punctuated(item, sep)),
                    Pair::End(item) => item
                        .find_and_replace(find, replace)
                        .map(|item| Pair::End(item)),
                })
                .collect::<Result<Vec<Pair<_, _>>>>()?,
        ))
    }
}

impl FindAndReplace for FieldSet {
    fn find_and_replace<T: Clone>(&self, find: &T, replace: &T) -> Result<Self> {
        Ok(match self.clone() {
            FieldSet::Named(struct_fields) => {
                FieldSet::Named(struct_fields.find_and_replace(find, replace)?)
            }
            FieldSet::Unnamed(struct_fields) => {
                FieldSet::Unnamed(struct_fields.find_and_replace(find, replace)?)
            }
            FieldSet::Unit(id, strict_name, help) => todo!(),
            FieldSet::Pure(expr) => todo!(),
        })
    }
}

impl FindAndReplace for Branch {
    fn find_and_replace<T: Clone>(&self, find: &T, replace: &T) -> Result<Self> {
        let Self {
            enum_name,
            ident,
            fields,
        } = self.clone();
        Ok(Self {
            enum_name,
            ident,
            fields: fields.find_and_replace(find, replace)?,
        })
    }
}

impl FindAndReplace for EnumBranch {
    fn find_and_replace<T: Clone>(&self, find: &T, replace: &T) -> Result<Self> {
        todo!()
    }
}

impl FindAndReplace for Body {
    fn find_and_replace<T: Clone>(&self, find: &T, replace: &T) -> Result<Self> {
        Ok(match self.clone() {
            Body::Single(branch) => Body::Single(branch.find_and_replace(find, replace)?),
            Body::Alternatives(id, enum_branches) => Body::Alternatives(
                id.clone(),
                enum_branches
                    .into_iter()
                    .map(|b| b.find_and_replace(find, replace))
                    .collect::<Result<_>>()?,
            ),
        })
    }
}
