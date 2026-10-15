use core::cell::RefCell;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::meta::ParseNestedMeta;
use syn::spanned::Spanned;
use syn::{Error, Expr, ExprLit, Ident, ItemStruct, Lit, LitStr, Member, Path};

pub(super) struct Ctxt {
    errors: RefCell<Vec<Error>>,
}

impl Ctxt {
    fn capture<T>(&self, res: Result<T, Error>) -> Result<T, ()> {
        match res {
            Ok(val) => Ok(val),
            Err(err) => {
                self.errors.borrow_mut().push(err);
                Err(())
            }
        }
    }

    fn error(&self, err: Error) {
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

    let errors = 'errors: {
        if let Ok(stream) = inner(&cx, input) {
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

fn inner(cx: &Ctxt, input: TokenStream) -> Result<TokenStream, ()> {
    let st: ItemStruct = cx.capture(syn::parse2(input))?;

    let mut read_only = false;
    let mut crate_path: Path = syn::parse_quote!(::sqll);

    for a in &st.attrs {
        if !a.path().is_ident("sql") {
            continue;
        }

        let result = a.parse_nested_meta(|s: ParseNestedMeta<'_>| {
            if s.path.is_ident("read_only") {
                read_only = true;
                return Ok(());
            }

            if s.path.is_ident("crate") {
                crate_path = s.value()?.parse()?;
                return Ok(());
            }

            Err(Error::new_spanned(
                &s.path,
                "unsupported attribute, expected `read_only` or `crate`",
            ))
        });

        if let Err(error) = result {
            cx.error(error);
        }
    }

    let connection = quote!(#crate_path::Connection);
    let pool_error = quote!(#crate_path::PoolError);
    let send_statement = quote!(#crate_path::SendStatement);
    let is_read_only_t = quote!(#crate_path::IsReadOnly);
    let statements_t = quote!(#crate_path::Statements);
    let try_from_t = quote!(::core::convert::TryFrom);

    let mut assert_read_only_impl = None;
    let mut members = Vec::new();
    let mut variables = Vec::new();
    let mut builders = Vec::new();

    for (index, f) in st.fields.iter().enumerate() {
        let span = match f.ident {
            Some(ref name) => name.span(),
            None => f.span(),
        };

        let member = match f.ident {
            Some(ref name) => Member::from(name.clone()),
            None => Member::from(index),
        };

        match f.ident {
            Some(ref name) => {
                variables.push(Ident::new(&format!("__sqll_{name}"), span));
            }
            None => {
                variables.push(Ident::new(&format!("_{index}"), span));
            }
        }

        let mut query = QueryString::default();
        let mut statements = false;

        for a in &f.attrs {
            if !a.path().is_ident("sql") {
                continue;
            }

            match &a.meta {
                syn::Meta::Path(..) => {
                    cx.error(Error::new_spanned(
                        &a.meta,
                        "unsupported attribute, expected `statements` or a string literal",
                    ));

                    continue;
                }
                syn::Meta::List(list) => {
                    let result = list.parse_nested_meta(|s: ParseNestedMeta<'_>| {
                        if s.path.is_ident("statements") {
                            statements = true;
                            return Ok(());
                        }

                        Err(Error::new_spanned(
                            &s.path,
                            "unsupported attribute, expected `statements` or a string literal",
                        ))
                    });

                    if let Err(error) = result {
                        cx.error(error);
                    }

                    continue;
                }
                syn::Meta::NameValue(name_value) => {
                    if let Expr::Lit(ExprLit {
                        lit: Lit::Str(lit), ..
                    }) = &name_value.value
                    {
                        query.append(lit);
                        continue;
                    }

                    cx.error(Error::new_spanned(name_value, "expected string literal"));
                }
            }
        }

        members.push(member);

        let ty = &f.ty;

        let (read_only_method, prepare_failed, not_thread_safe, build_failed, member) =
            match f.ident {
                Some(ref name) => {
                    let message = name.to_string();

                    (
                        Ident::new("field_not_read_only", Span::call_site()),
                        Ident::new("field_prepare_failed", Span::call_site()),
                        Ident::new("field_not_thread_safe", Span::call_site()),
                        Ident::new("field_build_failed", Span::call_site()),
                        quote!(#message),
                    )
                }
                None => (
                    Ident::new("index_not_read_only", Span::call_site()),
                    Ident::new("index_prepare_failed", Span::call_site()),
                    Ident::new("index_not_thread_safe", Span::call_site()),
                    Ident::new("index_build_failed", Span::call_site()),
                    quote!(#index),
                ),
            };

        let read_only_check = read_only.then(|| {
            quote! {
                if !stmt.is_read_only() {
                    return Err(#pool_error::#read_only_method(#member));
                }
            }
        });

        if statements {
            if let Some(span) = query.span {
                cx.error(Error::new(
                    span,
                    "cannot have both `statements` and a query string",
                ));
            }

            // A `read_only` collection can only embed other read-only
            // collections, otherwise its `IsReadOnly` implementation would be
            // unsound since the per-statement checks below are bypassed for
            // nested statements.
            let assert_read_only = read_only.then(|| {
                quote! {
                    let _ = _assert_is_read_only::<#ty>;
                }
            });

            if assert_read_only_impl.is_none() {
                assert_read_only_impl = Some(quote! {
                    fn _assert_is_read_only<T: #is_read_only_t>() {}
                });
            }

            builders.push(quote! {{
                #assert_read_only
                <#ty as #statements_t>::build(c)?
            }});
        } else {
            let query = query.output;

            builders.push(quote! {{
                let stmt = match c.prepare_with(#query).persistent().build() {
                    Ok(stmt) => stmt,
                    Err(error) => {
                        return Err(#pool_error::#prepare_failed(#member, error));
                    }
                };

                let stmt = match unsafe { stmt.into_send() } {
                    Ok(stmt) => stmt,
                    Err(error) => {
                        return Err(#pool_error::#not_thread_safe(#member, error));
                    }
                };

                #read_only_check

                match <#ty as #try_from_t<#send_statement>>::try_from(stmt) {
                    Ok(stmt) => stmt,
                    Err(error) => {
                        return Err(#pool_error::#build_failed(#member, error));
                    }
                }
            }});
        }
    }

    let ident = &st.ident;

    let impl_read_only = read_only.then(|| {
        quote! {
            #[automatically_derived]
            unsafe impl #is_read_only_t for #ident {}
        }
    });

    Ok(quote! {
        #[automatically_derived]
        impl #statements_t for #ident {
            fn build(c: &mut #connection) -> Result<Self, #pool_error> {
                #assert_read_only_impl

                #(let #variables = #builders;)*

                Ok(Self {
                    #(#members: #variables,)*
                })
            }
        }

        #impl_read_only
    })
}

#[derive(Default)]
struct QueryString {
    output: String,
    span: Option<Span>,
}

impl QueryString {
    fn append(&mut self, s: &LitStr) {
        self.span = Some(s.span());

        let s = s.value();
        let s = s.trim();

        if s.is_empty() {
            return;
        }

        if !self.output.is_empty() {
            self.output.push(' ');
        }

        self.output.push_str(s);
    }
}

#[cfg(test)]
mod tests {
    use proc_macro2::{TokenStream, TokenTree};
    use quote::quote;

    fn idents(stream: TokenStream, out: &mut Vec<String>) {
        for tt in stream {
            match tt {
                TokenTree::Ident(ident) => out.push(ident.to_string()),
                TokenTree::Group(group) => idents(group.stream(), out),
                _ => {}
            }
        }
    }

    fn expand(input: TokenStream) -> Vec<String> {
        let mut out = Vec::new();
        idents(super::expand(input), &mut out);
        assert!(
            !out.iter().any(|i| i == "compile_error"),
            "expansion failed: {out:?}"
        );
        out
    }

    #[test]
    fn default_crate_path() {
        let out = expand(quote! {
            #[sql(read_only)]
            struct Read {
                #[sql = "SELECT 1"]
                one: SendStatement,
            }
        });

        assert!(out.iter().any(|i| i == "sqll"));
    }

    #[test]
    fn renamed_crate_path() {
        let read = expand(quote! {
            #[sql(read_only, crate = renamed)]
            struct Read {
                #[sql = "SELECT 1"]
                one: SendStatement,
                #[sql(statements)]
                inner: Inner,
            }
        });

        let write = expand(quote! {
            #[sql(crate = renamed)]
            struct Write {
                #[sql(statements)]
                read: Read,
                #[sql = "CREATE TABLE foo (id INTEGER)"]
                create_table: SendStatement,
            }
        });

        for out in [read, write] {
            assert!(out.iter().any(|i| i == "renamed"));
            assert!(!out.iter().any(|i| i == "sqll"), "{out:?}");
        }
    }
}
