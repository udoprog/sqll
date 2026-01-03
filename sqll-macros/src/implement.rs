use core::cell::RefCell;
use core::ffi::c_int;
use core::fmt;

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::{
    Data, DataStruct, DeriveInput, Error, Ident, Index, Lifetime, LifetimeParam, LitInt, Member,
    Path, PathArguments, PathSegment,
};

#[derive(Clone, Copy)]
pub(super) enum What {
    Bind,
    FromRow,
}

impl What {
    fn start_index(&self) -> usize {
        match self {
            What::Bind => 1,
            What::FromRow => 0,
        }
    }
}

struct Tokens<'a> {
    bind_t: TypePath<'a, 1>,
    bind_value_t: TypePath<'a, 1>,
    error: TypePath<'a, 1>,
    from_column_t: TypePath<'a, 1>,
    from_row_t: TypePath<'a, 1>,
    result: TypePath<'a, 2>,
    statement: TypePath<'a, 1>,
}

struct TypePath<'a, const N: usize> {
    base: &'a Path,
    ident: [&'a str; N],
}

impl<'a, const N: usize> TypePath<'a, N> {
    #[inline]
    fn new(base: &'a Path, ident: [&'a str; N]) -> Self {
        Self { base, ident }
    }
}

impl<const N: usize> ToTokens for TypePath<'_, N> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut path = self.base.clone();

        for ident in &self.ident {
            path.segments.push(PathSegment {
                ident: Ident::new(ident, Span::call_site()),
                arguments: PathArguments::None,
            });
        }

        path.to_tokens(tokens);
    }
}

impl<'a> Tokens<'a> {
    fn new(crate_path: &'a Path, core_path: &'a Path) -> Self {
        Self {
            bind_t: TypePath::new(crate_path, ["Bind"]),
            bind_value_t: TypePath::new(crate_path, ["BindValue"]),
            error: TypePath::new(crate_path, ["Error"]),
            from_column_t: TypePath::new(crate_path, ["FromColumn"]),
            from_row_t: TypePath::new(crate_path, ["FromRow"]),
            result: TypePath::new(core_path, ["result", "Result"]),
            statement: TypePath::new(crate_path, ["Statement"]),
        }
    }
}

pub(super) struct Ctxt {
    errors: RefCell<Vec<Error>>,
}

impl Ctxt {
    fn spanned(&self, span: impl ToTokens, message: impl fmt::Display) {
        let err = Error::new_spanned(span, message);
        self.errors.borrow_mut().push(err);
    }
}

fn cx() -> Ctxt {
    Ctxt {
        errors: RefCell::new(Vec::new()),
    }
}

pub(super) fn expand(input: TokenStream, what: What) -> TokenStream {
    let cx = cx();

    if let Ok(stream) = inner(&cx, input, what) {
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

fn inner(cx: &Ctxt, input: TokenStream, what: What) -> Result<TokenStream, ()> {
    let input: DeriveInput = match syn::parse2(input) {
        Ok(input) => input,
        Err(err) => {
            cx.errors.borrow_mut().push(err);
            return Err(());
        }
    };

    let mut crate_path = syn::parse_quote!(::sqll);
    let core_path = syn::parse_quote!(::core);

    for attr in &input.attrs {
        if !attr.path().is_ident("sql") {
            continue;
        }

        let result = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("crate") {
                crate_path = meta.value()?.parse()?;
                return Ok(());
            }

            Err(Error::new_spanned(
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

    let st = match &input.data {
        Data::Struct(data) => expand_struct(cx, data, what)?,
        _ => {
            cx.spanned(input.ident, "Row can only be derived for structs");
            return Err(());
        }
    };

    let ident = &input.ident;

    let Tokens {
        bind_t,
        bind_value_t,
        error,
        from_column_t,
        from_row_t,
        result,
        statement,
    } = &tokens;

    let Struct { fields, indexes } = st;

    match what {
        What::Bind => {
            let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

            let expanded = quote! {
                #[automatically_derived]
                impl #impl_generics #bind_t for #ident #ty_generics #where_clause {
                    #[inline]
                    fn bind(&self, stmt: &mut #statement) -> #result<(), #error> {
                        #(#bind_value_t::bind_value(&self.#fields, stmt, #indexes)?;)*
                        #result::Ok(())
                    }
                }
            };

            Ok(expanded)
        }
        What::FromRow => {
            let mut impl_generics;

            let (lt, impl_generics) = match input.generics.lifetimes().next() {
                Some(param) => (param.lifetime.clone(), &input.generics),
                None => {
                    let lt = Lifetime::new("'__stmt", Span::call_site());

                    impl_generics = input.generics.clone();

                    impl_generics.params.push(From::from(LifetimeParam {
                        attrs: Vec::new(),
                        lifetime: lt.clone(),
                        colon_token: None,
                        bounds: Punctuated::new(),
                    }));

                    (lt, &impl_generics)
                }
            };

            let this = quote! {
                Self {
                    #(#fields: #from_column_t::<#lt>::from_column(stmt, #indexes)?,)*
                }
            };

            let (impl_generics, _, where_clause) = impl_generics.split_for_impl();
            let (_, ty_generics, _) = input.generics.split_for_impl();

            let expanded = quote! {
                #[automatically_derived]
                impl #impl_generics #from_row_t<#lt> for #ident #ty_generics #where_clause {
                    #[inline]
                    fn from_row(stmt: &#lt #statement) -> #result<Self, #error> {
                        #result::Ok(#this)
                    }
                }
            };

            Ok(expanded)
        }
    }
}

#[derive(Default)]
struct Struct {
    fields: Vec<Member>,
    indexes: Vec<c_int>,
}

fn expand_struct(cx: &Ctxt, data: &DataStruct, what: What) -> Result<Struct, ()> {
    let mut st = Struct::default();

    let mut index = what.start_index();

    for field in data.fields.iter() {
        for attr in &field.attrs {
            if !attr.path().is_ident("sql") {
                continue;
            }

            let result = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("index") {
                    let lit = meta.value()?.parse::<LitInt>()?;
                    index = lit.base10_parse::<usize>()?;
                    return Ok(());
                }

                Err(Error::new_spanned(
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
            Some(ident) => Member::Named(ident.clone()),
            None => Member::Unnamed(Index::from(index)),
        };

        let Ok(n) = c_int::try_from(index) else {
            cx.spanned(
                &field.ty,
                format_args!("The index {index} is too large for a c_int"),
            );
            return Err(());
        };

        st.fields.push(member);
        st.indexes.push(n);

        index = index.wrapping_add(1);
    }

    Ok(st)
}
