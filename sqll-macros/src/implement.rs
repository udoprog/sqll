use core::cell::RefCell;
use core::ffi::c_int;
use core::fmt;
use std::ffi::CString;

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::{
    Data, DataStruct, DeriveInput, Error, Ident, Index, Lifetime, LifetimeParam, LitCStr, LitInt,
    LitStr, Member, Path, PathArguments, PathSegment, Type,
};

#[derive(Clone, Copy)]
pub(super) enum What {
    Bind,
    Row,
}

impl What {
    fn offset_index(&self) -> isize {
        match self {
            What::Bind => 1,
            What::Row => 0,
        }
    }
}

struct Tokens<'a> {
    bind_t: TypePath<'a, 1>,
    bind_value_t: TypePath<'a, 1>,
    code: TypePath<'a, 1>,
    column_type_t: TypePath<'a, 2>,
    error: TypePath<'a, 1>,
    from_column_t: TypePath<'a, 1>,
    result: TypePath<'a, 2>,
    row_t: TypePath<'a, 1>,
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
            code: TypePath::new(crate_path, ["Code"]),
            column_type_t: TypePath::new(crate_path, ["ty", "ColumnType"]),
            error: TypePath::new(crate_path, ["Error"]),
            from_column_t: TypePath::new(crate_path, ["FromColumn"]),
            result: TypePath::new(core_path, ["result", "Result"]),
            row_t: TypePath::new(crate_path, ["Row"]),
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

    let errors = 'errors: {
        if let Ok(stream) = inner(&cx, input, what) {
            let errors = cx.errors.into_inner();

            if !errors.is_empty() {
                break 'errors errors;
            }

            return stream;
        }

        cx.errors.into_inner()
    };

    debug_assert!(!errors.is_empty());
    let mut out = TokenStream::new();

    for err in errors {
        out.extend(err.to_compile_error());
    }

    out
}

struct Attrs {
    crate_path: Path,
    core_path: Path,
    named: bool,
}

fn inner(cx: &Ctxt, input: TokenStream, what: What) -> Result<TokenStream, ()> {
    let input: DeriveInput = match syn::parse2(input) {
        Ok(input) => input,
        Err(err) => {
            cx.errors.borrow_mut().push(err);
            return Err(());
        }
    };

    let mut attrs = Attrs {
        crate_path: syn::parse_quote!(::sqll),
        core_path: syn::parse_quote!(::core),
        named: false,
    };

    for attr in &input.attrs {
        if !attr.path().is_ident("sql") {
            continue;
        }

        let result = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("crate") {
                attrs.crate_path = meta.value()?.parse()?;
                return Ok(());
            }

            if meta.path.is_ident("named") {
                attrs.named = true;
                return Ok(());
            }

            Err(Error::new_spanned(
                meta.path,
                "unknown attribute for `Row` derive",
            ))
        });

        if let Err(err) = result {
            cx.errors.borrow_mut().push(err);
            return Err(());
        }
    }

    let tokens = Tokens::new(&attrs.crate_path, &attrs.core_path);

    let st = match &input.data {
        Data::Struct(data) => expand_struct(cx, data, &attrs, what)?,
        _ => {
            cx.spanned(input.ident, "Row can only be derived for structs");
            return Err(());
        }
    };

    let ident = &input.ident;

    let Tokens {
        bind_t,
        bind_value_t,
        code,
        column_type_t,
        error,
        from_column_t,
        result,
        row_t,
        statement,
    } = &tokens;

    let Struct {
        fields,
        types,
        bindings,
    } = st;

    match what {
        What::Bind => {
            let bindings = fields
                .iter()
                .zip(bindings.iter())
                .map(|(field, binding)| match binding {
                    Binding::Index(n) => quote! {
                        #bind_value_t::bind_value(&self.#field, stmt, #n)?;
                    },
                    Binding::Name(name) => quote! {{
                        let Some(index) = stmt.bind_parameter_index(#name) else {
                            return #result::Err(#error::new(#code::MISMATCH, format_args!("bad parameter name {:?}", #name)));
                        };

                        #bind_value_t::bind_value(&self.#field, stmt, index)?;
                    }},
                });

            let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

            let expanded = quote! {
                #[automatically_derived]
                impl #impl_generics #bind_t for #ident #ty_generics #where_clause {
                    #[inline]
                    fn bind(&self, stmt: &mut #statement) -> #result<(), #error> {
                        #(#bindings)*
                        #result::Ok(())
                    }
                }
            };

            Ok(expanded)
        }
        What::Row => {
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

            for binding in &bindings {
                if let Binding::Name(name) = binding {
                    cx.spanned(name, "cannot use named bindings when deriving Row");
                }
            }

            let mut setup = Vec::new();
            let mut checked = Vec::new();

            for (i, (b, ty)) in bindings.iter().zip(types).enumerate() {
                let Binding::Index(index) = b else {
                    continue;
                };

                let c = quote::format_ident!("v{i}");

                setup.push(quote! {
                    let #c = <<#ty as #from_column_t::<#lt>>::Type as #column_type_t>::check(stmt, #index)?;
                });

                checked.push(c);
            }

            let fields = fields.iter().zip(checked.iter()).map(|(m, c)| {
                quote! {
                    #m: #from_column_t::<#lt>::from_column(stmt, #c)?
                }
            });

            let (impl_generics, _, where_clause) = impl_generics.split_for_impl();
            let (_, ty_generics, _) = input.generics.split_for_impl();

            let expanded = quote! {
                #[automatically_derived]
                impl #impl_generics #row_t<#lt> for #ident #ty_generics #where_clause {
                    #[inline]
                    fn from_row(stmt: &#lt mut #statement) -> #result<Self, #error> {
                        #(#setup)*
                        #result::Ok(Self { #(#fields),* })
                    }
                }
            };

            Ok(expanded)
        }
    }
}

enum Binding {
    Index(c_int),
    Name(LitCStr),
}

#[derive(Default)]
struct Struct {
    fields: Vec<Member>,
    types: Vec<Type>,
    bindings: Vec<Binding>,
}

enum Name {
    None,
    LitStr(LitStr),
    LitCStr(LitCStr),
}

impl Name {
    fn existing(&self) -> Option<Span> {
        match self {
            Name::None => None,
            Name::LitStr(s) => Some(s.span()),
            Name::LitCStr(s) => Some(s.span()),
        }
    }
}

fn expand_struct(cx: &Ctxt, data: &DataStruct, attrs: &Attrs, what: What) -> Result<Struct, ()> {
    let mut st = Struct::default();

    let mut index = 0;

    for field in data.fields.iter() {
        let mut name = Name::None;

        for attr in &field.attrs {
            if !attr.path().is_ident("sql") {
                continue;
            }

            let result = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("index") {
                    index = meta.value()?.parse::<LitInt>()?.base10_parse::<usize>()?;
                    return Ok(());
                }

                if meta.path.is_ident("name") {
                    let value = meta.value()?;

                    if let Some(span) = name.existing() {
                        return Err(Error::new(span, "duplicate `name` attribute for field"));
                    }

                    if let Some(s) = value.parse()? {
                        name = Name::LitStr(s);
                    } else {
                        name = Name::LitCStr(value.parse()?);
                    }

                    return Ok(());
                }

                Err(Error::new_spanned(
                    meta.path,
                    "unknown attribute for `Row` derive",
                ))
            });

            if let Err(err) = result {
                cx.errors.borrow_mut().push(err);
                return Err(());
            }
        }

        let name = match (what, name, &field.ident) {
            (What::Bind, Name::LitCStr(name), _) => Some(name),
            (What::Bind, Name::LitStr(name), _) => {
                let Ok(c_str) = CString::new(name.value()) else {
                    cx.spanned(
                        &name,
                        format_args!(
                            "custom field name {:?} contains interior null byte",
                            name.value()
                        ),
                    );
                    continue;
                };

                Some(LitCStr::new(&c_str, name.span()))
            }
            (What::Bind, Name::None, Some(ident)) if attrs.named => {
                let name = format!(":{ident}");

                let Ok(c_str) = CString::new(name.clone()) else {
                    cx.spanned(
                        ident,
                        format_args!("custom field name {name:?} contains interior null byte"),
                    );
                    continue;
                };

                Some(LitCStr::new(&c_str, ident.span()))
            }
            (What::Bind, Name::None, None) if attrs.named => {
                cx.spanned(&field.ty, "named fields require field names derive");
                continue;
            }
            _ => None,
        };

        let member = match &field.ident {
            Some(ident) => Member::Named(ident.clone()),
            None => Member::Unnamed(Index::from(index)),
        };

        let access = match name {
            Some(name) => Binding::Name(name),
            None => {
                let Some(n) = index.checked_add_signed(what.offset_index()) else {
                    cx.spanned(field, "index is out of bounds for the derived type");
                    continue;
                };

                let Ok(n) = c_int::try_from(n) else {
                    cx.spanned(
                        field,
                        format_args!("underlying index {n} is too large for a c_int"),
                    );
                    continue;
                };

                index = index.wrapping_add(1);
                Binding::Index(n)
            }
        };

        st.fields.push(member);
        st.types.push(field.ty.clone());
        st.bindings.push(access);
    }

    Ok(st)
}
