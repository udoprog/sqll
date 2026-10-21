//! The `FromColumn` and `BindValue` derives.
//!
//! Both derives support two shapes:
//!
//! * A struct with exactly one field, which delegates to that field.
//! * A fieldless enum stored either as an integer (`#[sql(integer)]`) or as
//!   text (`#[sql(text)]`).

use std::collections::HashMap;

use proc_macro2::{Literal, Span, TokenStream};
use quote::{ToTokens, quote};
use syn::spanned::Spanned;
use syn::{
    Data, DataEnum, DataStruct, DeriveInput, Error, Expr, ExprLit, ExprUnary, Fields, Generics,
    Ident, Lifetime, LifetimeParam, Lit, LitStr, Member, Path, Type, UnOp, Variant,
};

#[derive(Clone, Copy)]
pub(super) enum What {
    FromColumn,
    BindValue,
}

impl What {
    fn name(self) -> &'static str {
        match self {
            What::FromColumn => "FromColumn",
            What::BindValue => "BindValue",
        }
    }
}

pub(super) fn expand(input: TokenStream, what: What) -> TokenStream {
    let mut errors = Vec::new();

    match inner(&mut errors, input, what) {
        Some(stream) if errors.is_empty() => stream,
        _ => {
            let mut out = TokenStream::new();

            for err in errors {
                out.extend(err.to_compile_error());
            }

            out
        }
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Integer,
    Text,
}

#[derive(Clone, Copy)]
enum RenameAll {
    Lower,
    Upper,
    Snake,
    ScreamingSnake,
    Kebab,
}

impl RenameAll {
    const EXPECTED: &'static str = "expected one of \"lowercase\", \"UPPERCASE\", \"snake_case\", \"SCREAMING_SNAKE_CASE\" or \"kebab-case\"";

    fn parse(s: &LitStr) -> Result<Self, Error> {
        Ok(match s.value().as_str() {
            "lowercase" => RenameAll::Lower,
            "UPPERCASE" => RenameAll::Upper,
            "snake_case" => RenameAll::Snake,
            "SCREAMING_SNAKE_CASE" => RenameAll::ScreamingSnake,
            "kebab-case" => RenameAll::Kebab,
            _ => {
                return Err(Error::new(
                    s.span(),
                    format_args!("unsupported `rename_all` value, {}", Self::EXPECTED),
                ));
            }
        })
    }

    fn apply(self, ident: &str) -> String {
        match self {
            RenameAll::Lower => ident.to_lowercase(),
            RenameAll::Upper => ident.to_uppercase(),
            RenameAll::Snake => separated(ident, '_').to_lowercase(),
            RenameAll::ScreamingSnake => separated(ident, '_').to_uppercase(),
            RenameAll::Kebab => separated(ident, '-').to_lowercase(),
        }
    }
}

/// Insert `sep` before every upper case character which is not the first one,
/// turning `InProgress` into `In_Progress`.
fn separated(ident: &str, sep: char) -> String {
    let mut out = String::with_capacity(ident.len() + 4);

    for (i, c) in ident.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(sep);
        }

        out.push(c);
    }

    out
}

struct Attrs {
    crate_path: Path,
    mode: Option<(Mode, Span)>,
    rename_all: Option<(RenameAll, Span)>,
    transparent: Option<Span>,
}

fn inner(errors: &mut Vec<Error>, input: TokenStream, what: What) -> Option<TokenStream> {
    let input: DeriveInput = match syn::parse2(input) {
        Ok(input) => input,
        Err(err) => {
            errors.push(err);
            return None;
        }
    };

    let mut attrs = Attrs {
        crate_path: syn::parse_quote!(::sqll),
        mode: None,
        rename_all: None,
        transparent: None,
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

            if meta.path.is_ident("transparent") {
                attrs.transparent = Some(meta.path.span());
                return Ok(());
            }

            let mode = if meta.path.is_ident("integer") {
                Some(Mode::Integer)
            } else if meta.path.is_ident("text") {
                Some(Mode::Text)
            } else {
                None
            };

            if let Some(mode) = mode {
                if attrs.mode.is_some() {
                    return Err(Error::new_spanned(
                        &meta.path,
                        "only one of `integer` or `text` may be specified",
                    ));
                }

                attrs.mode = Some((mode, meta.path.span()));
                return Ok(());
            }

            if meta.path.is_ident("rename_all") {
                let s: LitStr = meta.value()?.parse()?;
                attrs.rename_all = Some((RenameAll::parse(&s)?, meta.path.span()));
                return Ok(());
            }

            Err(Error::new_spanned(
                &meta.path,
                "unsupported attribute, expected `crate`, `transparent`, `integer`, `text` or `rename_all`",
            ))
        });

        if let Err(err) = result {
            errors.push(err);
            return None;
        }
    }

    let tokens = Tokens::new(&attrs.crate_path);

    match &input.data {
        Data::Struct(data) => {
            if let Some((_, span)) = attrs.mode {
                errors.push(Error::new(
                    span,
                    "`integer` and `text` are only supported on enums, a struct delegates to its field",
                ));
            }

            if let Some((_, span)) = attrs.rename_all {
                errors.push(Error::new(span, "`rename_all` is only supported on enums"));
            }

            expand_struct(errors, &input, data, &tokens, what)
        }
        Data::Enum(data) => {
            if let Some(span) = attrs.transparent {
                errors.push(Error::new(
                    span,
                    "`transparent` is only supported on single-field structs",
                ));
            }

            expand_enum(errors, &input, data, &attrs, &tokens, what)
        }
        Data::Union(..) => {
            errors.push(Error::new_spanned(
                &input.ident,
                format_args!(
                    "`{}` can only be derived for single-field structs and fieldless enums",
                    what.name()
                ),
            ));
            None
        }
    }
}

struct Tokens<'a> {
    base: &'a Path,
}

impl<'a> Tokens<'a> {
    fn new(base: &'a Path) -> Self {
        Self { base }
    }

    fn path(&self, segments: &[&str]) -> Path {
        let mut path = self.base.clone();

        for s in segments {
            path.segments.push(Ident::new(s, Span::call_site()).into());
        }

        path
    }
}

/// Reject any `#[sql(..)]` attribute in `attrs`.
fn reject_sql_attrs(errors: &mut Vec<Error>, attrs: &[syn::Attribute], what: &str) {
    for attr in attrs {
        if attr.path().is_ident("sql") {
            errors.push(Error::new_spanned(
                attr,
                format_args!("`#[sql(..)]` attributes are not supported on {what}"),
            ));
        }
    }
}

/// Get the lifetime to use for `FromColumn<'stmt>`, either the first lifetime
/// parameter of the type or a fresh one which is added to the impl generics.
fn stmt_lifetime(generics: &Generics) -> (Lifetime, Generics) {
    let mut impl_generics = generics.clone();

    match generics.lifetimes().next() {
        Some(param) => (param.lifetime.clone(), impl_generics),
        None => {
            let lt = Lifetime::new("'__stmt", Span::call_site());
            impl_generics
                .params
                .push(LifetimeParam::new(lt.clone()).into());
            (lt, impl_generics)
        }
    }
}

fn expand_struct(
    errors: &mut Vec<Error>,
    input: &DeriveInput,
    data: &DataStruct,
    tokens: &Tokens<'_>,
    what: What,
) -> Option<TokenStream> {
    let mut fields = data.fields.iter();

    let (Some(field), None) = (fields.next(), fields.next()) else {
        errors.push(Error::new_spanned(
            &input.ident,
            format_args!(
                "`{}` can only be derived for structs with exactly one field, use `{}` for multiple columns",
                what.name(),
                match what {
                    What::FromColumn => "Row",
                    What::BindValue => "Bind",
                }
            ),
        ));
        return None;
    };

    reject_sql_attrs(errors, &field.attrs, "fields");

    let ident = &input.ident;
    let ty: &Type = &field.ty;

    let member = match &field.ident {
        Some(ident) => Member::Named(ident.clone()),
        None => Member::Unnamed(syn::Index::from(0)),
    };

    match what {
        What::FromColumn => {
            let result = tokens.path(&["Result"]);
            let statement = tokens.path(&["Statement"]);
            let from_column_t = tokens.path(&["FromColumn"]);
            let (lt, mut impl_generics) = stmt_lifetime(&input.generics);

            impl_generics
                .make_where_clause()
                .predicates
                .push(syn::parse_quote_spanned!(ty.span() => #ty: #from_column_t<#lt>));

            let (impl_generics, _, where_clause) = impl_generics.split_for_impl();
            let (_, ty_generics, _) = input.generics.split_for_impl();

            Some(quote! {
                #[automatically_derived]
                impl #impl_generics #from_column_t<#lt> for #ident #ty_generics #where_clause {
                    type Type = <#ty as #from_column_t<#lt>>::Type;

                    #[inline]
                    fn from_column(stmt: &#lt #statement, index: Self::Type) -> #result<Self> {
                        #result::Ok(Self {
                            #member: <#ty as #from_column_t<#lt>>::from_column(stmt, index)?,
                        })
                    }
                }
            })
        }
        What::BindValue => {
            let bind_value_t = tokens.path(&["BindValue"]);
            let mut generics = input.generics.clone();

            generics
                .make_where_clause()
                .predicates
                .push(syn::parse_quote_spanned!(ty.span() => #ty: #bind_value_t));

            let body = quote! {
                <#ty as #bind_value_t>::bind_value(&self.#member, stmt, index)
            };

            Some(bind_value_impls(tokens, ident, &generics, body))
        }
    }
}

/// Emit `BindValue` with the given body, and a `Bind` impl binding the single
/// value to the first parameter.
fn bind_value_impls(
    tokens: &Tokens<'_>,
    ident: &Ident,
    generics: &Generics,
    body: TokenStream,
) -> TokenStream {
    let bind_t = tokens.path(&["Bind"]);
    let bind_value_t = tokens.path(&["BindValue"]);
    let index_t = tokens.path(&["Index"]);
    let result = tokens.path(&["Result"]);
    let statement = tokens.path(&["Statement"]);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        #[automatically_derived]
        impl #impl_generics #bind_value_t for #ident #ty_generics #where_clause {
            #[inline]
            fn bind_value(&self, stmt: &mut #statement, index: #index_t) -> #result<()> {
                #body
            }
        }

        #[automatically_derived]
        impl #impl_generics #bind_t for #ident #ty_generics #where_clause {
            const COUNT: usize = 1;

            #[inline]
            fn bind(&self, stmt: &mut #statement) -> #result<()> {
                <Self as #bind_value_t>::bind_value(self, stmt, #index_t::BIND)
            }
        }
    }
}

/// The value a variant is stored as.
enum Value {
    Integer(i64),
    Text(String),
}

impl ToTokens for Value {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Value::Integer(n) => Literal::i64_suffixed(*n).to_tokens(tokens),
            Value::Text(s) => Literal::string(s).to_tokens(tokens),
        }
    }
}

struct VariantAttrs {
    value: Option<Expr>,
    rename: Option<LitStr>,
    other: Option<Span>,
}

fn variant_attrs(errors: &mut Vec<Error>, variant: &Variant) -> Option<VariantAttrs> {
    let mut attrs = VariantAttrs {
        value: None,
        rename: None,
        other: None,
    };

    for attr in &variant.attrs {
        if !attr.path().is_ident("sql") {
            continue;
        }

        let result = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("value") {
                attrs.value = Some(meta.value()?.parse()?);
                return Ok(());
            }

            if meta.path.is_ident("rename") {
                attrs.rename = Some(meta.value()?.parse()?);
                return Ok(());
            }

            if meta.path.is_ident("other") {
                attrs.other = Some(meta.path.span());
                return Ok(());
            }

            Err(Error::new_spanned(
                &meta.path,
                "unsupported attribute, expected `value`, `rename` or `other`",
            ))
        });

        if let Err(err) = result {
            errors.push(err);
            return None;
        }
    }

    Some(attrs)
}

/// Evaluate an integer literal, optionally negated.
fn int_literal(expr: &Expr) -> Result<i64, Error> {
    const MESSAGE: &str = "expected an integer literal, use `#[sql(value = ..)]` to give a value which is not a literal";

    let (negative, lit) = match expr {
        Expr::Lit(ExprLit { lit, .. }) => (false, lit),
        Expr::Unary(ExprUnary {
            op: UnOp::Neg(..),
            expr,
            ..
        }) => match &**expr {
            Expr::Lit(ExprLit { lit, .. }) => (true, lit),
            _ => return Err(Error::new_spanned(expr, MESSAGE)),
        },
        _ => return Err(Error::new_spanned(expr, MESSAGE)),
    };

    let Lit::Int(lit) = lit else {
        return Err(Error::new_spanned(lit, MESSAGE));
    };

    let n = lit.base10_parse::<i128>()?;
    let n = if negative { -n } else { n };

    i64::try_from(n)
        .map_err(|_| Error::new_spanned(expr, format_args!("value {n} does not fit in an i64")))
}

fn expand_enum(
    errors: &mut Vec<Error>,
    input: &DeriveInput,
    data: &DataEnum,
    attrs: &Attrs,
    tokens: &Tokens<'_>,
    what: What,
) -> Option<TokenStream> {
    let ident = &input.ident;

    let Some((mode, _)) = attrs.mode else {
        errors.push(Error::new_spanned(
            ident,
            format_args!(
                "deriving `{}` for an enum requires `#[sql(integer)]` or `#[sql(text)]` to pick how it is stored",
                what.name()
            ),
        ));
        return None;
    };

    if let (Mode::Integer, Some((_, span))) = (mode, attrs.rename_all) {
        errors.push(Error::new(
            span,
            "`rename_all` is only supported with `#[sql(text)]`",
        ));
    }

    let mut variants = Vec::new();
    let mut seen = HashMap::<String, Span>::new();
    let mut other = None::<(Ident, Span)>;
    // The next implicit integer value, or `None` if it cannot be determined
    // because the previous value overflowed or wasn't a literal.
    let mut next = Some(0i64);

    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            errors.push(Error::new_spanned(
                &variant.fields,
                format_args!(
                    "`{}` can only be derived for enums whose variants have no fields",
                    what.name()
                ),
            ));
            next = None;
            continue;
        }

        let Some(v) = variant_attrs(errors, variant) else {
            next = None;
            continue;
        };

        if let Some(span) = v.other {
            if let Some((_, existing)) = &other {
                let mut err = Error::new(span, "only one variant can be marked `other`");
                err.combine(Error::new(*existing, "first marked `other` here"));
                errors.push(err);
            } else {
                other = Some((variant.ident.clone(), span));
            }
        }

        let value = match mode {
            Mode::Integer => {
                if let Some(rename) = &v.rename {
                    errors.push(Error::new_spanned(
                        rename,
                        "`rename` is only supported with `#[sql(text)]`, use `value` with `#[sql(integer)]`",
                    ));
                }

                let explicit = v
                    .value
                    .as_ref()
                    .or(variant.discriminant.as_ref().map(|(_, e)| e));

                let n = match explicit {
                    Some(expr) => match int_literal(expr) {
                        Ok(n) => n,
                        Err(err) => {
                            errors.push(err);
                            next = None;
                            continue;
                        }
                    },
                    None => match next {
                        Some(n) => n,
                        None => {
                            errors.push(Error::new_spanned(
                                &variant.ident,
                                "cannot infer the value of this variant, give it with `#[sql(value = ..)]`",
                            ));
                            continue;
                        }
                    },
                };

                next = n.checked_add(1);
                Value::Integer(n)
            }
            Mode::Text => {
                if let Some(value) = &v.value {
                    errors.push(Error::new_spanned(
                        value,
                        "`value` is only supported with `#[sql(integer)]`, use `rename` with `#[sql(text)]`",
                    ));
                }

                let name = match (&v.rename, attrs.rename_all) {
                    (Some(rename), _) => rename.value(),
                    (None, Some((rename_all, _))) => rename_all.apply(&variant.ident.to_string()),
                    (None, None) => variant.ident.to_string(),
                };

                Value::Text(name)
            }
        };

        let key = match &value {
            Value::Integer(n) => n.to_string(),
            Value::Text(s) => format!("{s:?}"),
        };

        let span = v
            .rename
            .as_ref()
            .map(|r| r.span())
            .or(v.value.as_ref().map(|v| v.span()))
            .unwrap_or(variant.ident.span());

        if let Some(existing) = seen.insert(key.clone(), span) {
            let mut err = Error::new(
                span,
                format_args!(
                    "variant `{}` has the same value {key} as another variant",
                    variant.ident
                ),
            );
            err.combine(Error::new(existing, "first used here"));
            errors.push(err);
        }

        variants.push((&variant.ident, value));
    }

    if !errors.is_empty() {
        return None;
    }

    match what {
        What::FromColumn => {
            let result = tokens.path(&["Result"]);
            let statement = tokens.path(&["Statement"]);
            let from_column_t = tokens.path(&["FromColumn"]);
            let error = tokens.path(&["Error"]);
            let (lt, impl_generics) = stmt_lifetime(&input.generics);

            let (ty, read) = match mode {
                Mode::Integer => (tokens.path(&["ty", "Integer"]), quote!(i64)),
                Mode::Text => (tokens.path(&["ty", "Text"]), quote!(&#lt str)),
            };

            let arms = variants
                .iter()
                .map(|(variant, value)| quote!(#value => #result::Ok(Self::#variant),));

            let fallback = match &other {
                Some((variant, _)) => quote!(_ => #result::Ok(Self::#variant),),
                None => {
                    let message = match mode {
                        Mode::Integer => format!("unknown {ident} value {{}}"),
                        Mode::Text => format!("unknown {ident} value {{:?}}"),
                    };

                    quote! {
                        value => #result::Err(#error::mismatch(::core::format_args!(#message, value))),
                    }
                }
            };

            let (impl_generics, _, where_clause) = impl_generics.split_for_impl();
            let (_, ty_generics, _) = input.generics.split_for_impl();

            Some(quote! {
                #[automatically_derived]
                impl #impl_generics #from_column_t<#lt> for #ident #ty_generics #where_clause {
                    type Type = #ty;

                    #[inline]
                    fn from_column(stmt: &#lt #statement, index: #ty) -> #result<Self> {
                        match <#read as #from_column_t<#lt>>::from_column(stmt, index)? {
                            #(#arms)*
                            #fallback
                        }
                    }
                }
            })
        }
        What::BindValue => {
            let bind_value_t = tokens.path(&["BindValue"]);

            let read = match mode {
                Mode::Integer => quote!(i64),
                Mode::Text => quote!(&'static str),
            };

            let arms = variants
                .iter()
                .map(|(variant, value)| quote!(Self::#variant => #value,));

            let body = quote! {
                let value: #read = match self {
                    #(#arms)*
                };

                #bind_value_t::bind_value(&value, stmt, index)
            };

            Some(bind_value_impls(tokens, ident, &input.generics, body))
        }
    }
}
