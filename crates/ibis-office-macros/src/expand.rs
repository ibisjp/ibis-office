// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The expansion of `#[editor_ext(Name)]`, on `proc_macro2` tokens so that the tests can call it.
use proc_macro2::{Group, Span, TokenStream};
use quote::{format_ident, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Attribute, FnArg, Ident, Pat, Signature, Token, Type, Visibility};

/// The attributes a method may carry, and where each goes. Any other is an error: the macro cannot know whether it belongs
/// on the declaration in the trait, on the method of the impl, or on both.
const ON_BOTH: &[&str] = &["cfg", "cfg_attr", "doc", "allow", "expect", "warn", "deny", "forbid", "track_caller"];
/// The compiler refuses these on a method without a body.
const ON_METHOD: &[&str] = &["inline", "cold"];
/// The compiler refuses these on a method of the impl of a trait (they are read from the trait).
const ON_DECLARATION: &[&str] = &["must_use", "deprecated"];

struct Block {
    attrs: Vec<Attribute>,
    self_ty: Type,
    methods: Vec<Method>,
}

struct Method {
    attrs: Vec<Attribute>,
    vis: Visibility,
    sig: Signature,
    body: Group,
}

impl Parse for Block {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        if input.peek(Token![unsafe]) {
            return Err(input.error("#[editor_ext]: `unsafe impl` is not supported"));
        }
        if !input.peek(Token![impl]) {
            return Err(input.error("#[editor_ext] goes on `impl Type { fn .. }`"));
        }
        input.parse::<Token![impl]>()?;
        if input.peek(Token![<]) {
            return Err(input.error("#[editor_ext]: an impl with parameters (`impl<T>`) is not supported"));
        }
        let self_ty: Type = input.parse()?;
        if input.peek(Token![for]) {
            return Err(input.error("#[editor_ext] goes on the methods of a type (`impl Type { .. }`), not on the impl of a trait"));
        }
        if input.peek(Token![where]) {
            return Err(input.error("#[editor_ext]: a `where` on the impl is not supported"));
        }
        let content;
        syn::braced!(content in input);
        let mut methods = Vec::new();
        while !content.is_empty() {
            methods.push(content.parse()?);
        }
        Ok(Block { attrs, self_ty, methods })
    }
}

impl Parse for Method {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        for attr in &attrs {
            let known = [ON_BOTH, ON_METHOD, ON_DECLARATION].iter().any(|list| list.iter().any(|name| attr.path().is_ident(name)));
            if !known {
                return Err(syn::Error::new(attr.span(), "#[editor_ext] does not know where this attribute goes (the trait, the impl or both): see ibis-office-macros/src/expand.rs"));
            }
        }
        let vis: Visibility = input.parse()?;
        if input.peek(Token![const]) && (input.peek2(Token![fn]) || input.peek2(Token![async]) || input.peek2(Token![unsafe]) || input.peek2(Token![extern])) {
            return Err(input.error("#[editor_ext]: a method of a trait cannot be `const fn`; write it outside the block"));
        }
        if input.peek(Token![async]) || input.peek(Token![unsafe]) || input.peek(Token![extern]) {
            return Err(input.error("#[editor_ext]: `async`, `unsafe` and `extern` methods are not supported"));
        }
        if !input.peek(Token![fn]) {
            return Err(input.error("#[editor_ext]: only methods (`fn`) can be written in this block; a constant or a type goes outside it"));
        }
        let sig: Signature = input.parse()?;
        if let Some(variadic) = &sig.variadic {
            return Err(syn::Error::new(variadic.span(), "#[editor_ext]: a variadic method is not supported"));
        }
        if !input.peek(syn::token::Brace) {
            return Err(input.error("#[editor_ext]: a method needs its body"));
        }
        let body: Group = input.parse()?;
        Ok(Method { attrs, vis, sig, body })
    }
}

/// What the name of the trait ends with, for the visibility written on a method.
fn suffix(vis: &Visibility) -> &'static str {
    match vis {
        Visibility::Public(_) => "",
        Visibility::Inherited => "Private",
        Visibility::Restricted(r) if r.in_token.is_none() && r.path.is_ident("self") => "Private",
        Visibility::Restricted(r) if r.in_token.is_none() && r.path.is_ident("crate") => "Crate",
        Visibility::Restricted(r) if r.in_token.is_none() && r.path.is_ident("super") => "Super",
        Visibility::Restricted(_) => "In",
    }
}

/// The signature for the trait: a method without a body takes no patterns (`mut x` is `x` there, `(a, b)` is `_`).
fn declaration(sig: &Signature) -> Signature {
    let mut sig = sig.clone();
    for arg in sig.inputs.iter_mut() {
        match arg {
            FnArg::Receiver(r) => {
                if r.reference.is_none() {
                    r.mutability = None;
                }
            }
            FnArg::Typed(t) => match &mut *t.pat {
                Pat::Ident(p) => {
                    p.mutability = None;
                    p.by_ref = None;
                    p.subpat = None;
                }
                _ => *t.pat = syn::parse_quote!(_),
            },
        }
    }
    capture_self(&mut sig);
    sig
}

/// In a trait, a list of what the returned `impl Trait` captures has to name `Self` (`use<>` is refused there): the
/// declaration gets `use<Self>`, the method in the impl keeps what was written.
fn capture_self(sig: &mut Signature) {
    let syn::ReturnType::Type(_, ty) = &mut sig.output else { return };
    let Type::ImplTrait(it) = &mut **ty else { return };
    for bound in it.bounds.iter_mut() {
        if let syn::TypeParamBound::PreciseCapture(list) = bound {
            let has = list.params.iter().any(|p| matches!(p, syn::CapturedParam::Ident(i) if i == "Self"));
            if !has {
                // Lifetimes come first in the list.
                let at = list.params.iter().take_while(|p| matches!(p, syn::CapturedParam::Lifetime(_))).count();
                list.params.insert(at, syn::CapturedParam::Ident(Ident::new("Self", list.use_token.span)));
            }
        }
    }
}

fn is_one_of(attr: &Attribute, names: &[&str]) -> bool {
    names.iter().any(|name| attr.path().is_ident(name))
}

fn expand(name: &Ident, block: Block) -> syn::Result<TokenStream> {
    let Block { attrs: block_attrs, self_ty, methods } = block;
    // Every token written here takes the place of the name in the attribute: the compiler then treats the traits as code
    // of that file, and warns about a method nobody calls.
    let span = name.span();
    // One trait per visibility, in the order the visibilities first come in the block.
    let mut classes: Vec<(&'static str, Visibility, Vec<&Method>)> = Vec::new();
    for m in &methods {
        let s = suffix(&m.vis);
        match classes.iter_mut().find(|c| c.0 == s) {
            Some(c) => {
                // `pub(in a)` and `pub(in b)` would need a trait each.
                if s == "In" && visibility_text(&c.1) != visibility_text(&m.vis) {
                    return Err(syn::Error::new(m.vis.span(), "#[editor_ext]: the methods of one block can name one `pub(in ..)` path only"));
                }
                c.2.push(m)
            }
            None => classes.push((s, m.vis.clone(), vec![m])),
        }
    }
    let mut out = TokenStream::new();
    for (s, vis, ms) in classes {
        let trait_name = format_ident!("{}{}", name, s, span = span);
        let mut decls = TokenStream::new();
        let mut defs = TokenStream::new();
        for m in ms {
            let on_decl = m.attrs.iter().filter(|a| !is_one_of(a, ON_METHOD));
            let on_def = m.attrs.iter().filter(|a| !is_one_of(a, ON_DECLARATION));
            let decl = declaration(&m.sig);
            let sig = &m.sig;
            let body = &m.body;
            decls.extend(quote_spanned! {span=> #(#on_decl)* #decl; });
            defs.extend(quote_spanned! {span=> #(#on_def)* #sig #body });
        }
        out.extend(quote_spanned! {span=>
            #(#block_attrs)* #vis trait #trait_name: ::core::marker::Sized + 'static { #decls }
            #(#block_attrs)* impl #trait_name for #self_ty { #defs }
        });
    }
    Ok(out)
}

fn visibility_text(vis: &Visibility) -> String {
    quote::ToTokens::to_token_stream(vis).to_string()
}

/// The traits and their impls; or, where the block is not what the macro carries, the error at its place followed by the
/// block as it was written (so that the one error is not buried under "no method named .." for every call).
pub(crate) fn editor_ext(attr: TokenStream, item: TokenStream) -> TokenStream {
    match try_editor_ext(attr, item.clone()) {
        Ok(out) => out,
        Err(error) => {
            let mut out = error.to_compile_error();
            out.extend(item);
            out
        }
    }
}

pub(crate) fn try_editor_ext(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    if attr.is_empty() {
        return Err(syn::Error::new(Span::call_site(), "#[editor_ext(Name)]: the name the traits are made from is missing"));
    }
    let name: Ident = syn::parse2(attr).map_err(|e| syn::Error::new(e.span(), "#[editor_ext(Name)] takes one name (the traits are `Name`, `NameCrate`, `NamePrivate`, ..)"))?;
    let block: Block = syn::parse2(item)?;
    expand(&name, block)
}
