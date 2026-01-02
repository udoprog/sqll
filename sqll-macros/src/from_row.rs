use core::cell::RefCell;
use core::ffi::c_int;
use core::fmt;

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;

struct Tokens<'a> {
    error: TypePath<'a, 1>,
    from_row_t: TypePath<'a, 1>,
    gettable_t: TypePath<'a, 1>,
    result: TypePath<'a, 2>,
    row: TypePath<'a, 1>,
}

struct TypePath<'a, const N: usize> {
    base: &'a syn::Path,
    ident: [&'a str; N],
}

impl<'a, const N: usize> TypePath<'a, N> {
    #[inline]
    fn new(base: &'a syn::Path, ident: [&'a str; N]) -> Self {
        Self { base, ident }
    }
}

impl<const N: usize> ToTokens for TypePath<'_, N> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut path = self.base.clone();

        for ident in &self.ident {
            path.segments.push(syn::PathSegment {
                ident: syn::Ident::new(ident, Span::call_site()),
                arguments: syn::PathArguments::None,
            });
        }

        path.to_tokens(tokens);
    }
}

impl<'a> Tokens<'a> {
    fn new(crate_path: &'a syn::Path, core_path: &'a syn::Path) -> Self {
        Self {
            error: TypePath::new(crate_path, ["Error"]),
            from_row_t: TypePath::new(crate_path, ["FromRow"]),
            gettable_t: TypePath::new(crate_path, ["Gettable"]),
            result: TypePath::new(core_path, ["result", "Result"]),
            row: TypePath::new(crate_path, ["Row"]),
        }
    }
}

pub(super) struct Ctxt {
    errors: RefCell<Vec<syn::Error>>,
}

impl Ctxt {
    fn spanned(&self, span: impl ToTokens, message: impl fmt::Display) {
        let err = syn::Error::new_spanned(span, message);
        self.errors.borrow_mut().push(err);
    }
}

fn cx() -> Ctxt {
    Ctxt {
        errors: RefCell::new(Vec::new()),
    }
}

pub(super) fn expand(input: TokenStream) -> TokenStream {
    let cx = cx();

    if let Ok(stream) = inner(&cx, input) {
        return stream;
    }

    let errors = cx.errors.into_inner();
    debug_assert!(!errors.is_empty());

    let mut out = TokenStream::new();

    for err in errors {
        out.extend(err.to_compile_error());
    }

    out
}

fn inner(cx: &Ctxt, input: TokenStream) -> Result<TokenStream, ()> {
    let input: syn::DeriveInput = match syn::parse2(input) {
        Ok(input) => input,
        Err(err) => {
            cx.errors.borrow_mut().push(err);
            return Err(());
        }
    };

    let mut impl_generics;

    let (lt, impl_generics) = match input.generics.lifetimes().next() {
        Some(param) => (param.lifetime.clone(), &input.generics),
        None => {
            let lt = syn::Lifetime::new("'__stmt", Span::call_site());

            impl_generics = input.generics.clone();

            impl_generics.params.push(From::from(syn::LifetimeParam {
                attrs: Vec::new(),
                lifetime: lt.clone(),
                colon_token: None,
                bounds: Punctuated::new(),
            }));

            (lt, &impl_generics)
        }
    };

    let mut crate_path = syn::parse_quote!(::sqll);
    let core_path = syn::parse_quote!(::core);

    for attr in &input.attrs {
        if !attr.path().is_ident("from_row") {
            continue;
        }

        let result = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("crate") {
                crate_path = meta.value()?.parse()?;
                return Ok(());
            }

            Err(syn::Error::new_spanned(
                meta.path,
                "unknown attribute for `FromRow` derive",
            ))
        });

        if let Err(err) = result {
            cx.errors.borrow_mut().push(err);
            return Err(());
        }
    }

    let tokens = Tokens::new(&crate_path, &core_path);

    let expanded = match &input.data {
        syn::Data::Struct(data) => expand_struct(cx, &tokens, data, &lt)?,
        _ => {
            cx.spanned(input.ident, "Row can only be derived for structs");
            return Err(());
        }
    };

    let ident = &input.ident;

    let Tokens {
        error,
        result,
        from_row_t,
        row,
        ..
    } = &tokens;

    let (impl_generics, _, where_clause) = impl_generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();

    let expanded = quote! {
        #[automatically_derived]
        impl #impl_generics #from_row_t<#lt> for #ident #ty_generics #where_clause {
            #[inline]
            fn from_row(row: &#row<#lt>) -> #result<Self, #error> {
                #result::Ok(#expanded)
            }
        }
    };

    Ok(expanded)
}

fn expand_struct(
    cx: &Ctxt,
    tokens: &Tokens,
    data: &syn::DataStruct,
    lt: &syn::Lifetime,
) -> Result<TokenStream, ()> {
    let mut fields = Vec::new();
    let mut indexes = Vec::new();

    let mut index = 0;

    for field in data.fields.iter() {
        for attr in &field.attrs {
            if !attr.path().is_ident("from_row") {
                continue;
            }

            let result = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("index") {
                    let lit: syn::LitInt = meta.value()?.parse()?;
                    index = lit.base10_parse::<usize>()?;
                    return Ok(());
                }

                Err(syn::Error::new_spanned(
                    meta.path,
                    "unknown attribute for `FromRow` derive",
                ))
            });

            if let Err(err) = result {
                cx.errors.borrow_mut().push(err);
                return Err(());
            }
        }

        let member = match &field.ident {
            Some(ident) => syn::Member::Named(ident.clone()),
            None => syn::Member::Unnamed(syn::Index::from(index)),
        };

        let Ok(n) = c_int::try_from(index) else {
            cx.spanned(
                &field.ty,
                format_args!("The index {index} is too large for a c_int"),
            );
            return Err(());
        };

        fields.push(member);
        indexes.push(n);

        index = index.wrapping_add(1);
    }

    let Tokens {
        gettable_t, row, ..
    } = tokens;

    let this = quote! {
        Self {
            #(#fields: #gettable_t::<#lt>::get(#row::as_stmt(row), #indexes)?,)*
        }
    };

    Ok(this)
}
