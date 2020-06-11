use proc_macro2::TokenStream;
use proc_macro_error::{abort, abort_call_site, emit_error, proc_macro_error};
use quote::quote;
use syn::{
    parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input, token, Data, Ident, LitStr, Token,
};

#[proc_macro_error]
#[proc_macro_derive(Verify, attributes(verify))]
pub fn derive_verify(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);

    let mut options = VerifyOptions::default();

    for a in &input.attrs {
        if a.path.is_ident("verify") {
            let tokens = a.tokens.clone().into();
            options.merge(parse_macro_input!(tokens as VerifyOptions));
        }
    }

    match &input.data {
        Data::Struct(_) => Verify::new(input, options).derive().into(),
        Data::Enum(_) => Verify::new(input, options).derive().into(),
        Data::Union(u) => {
            abort!(u.union_token, "unions are not supported by Verify");
        }
    }
}

#[derive(Default)]
struct VerifyOptions {
    verifier: Option<Ident>,
    verifier_name: Option<(Ident, TokenStream)>,
    verifier_create: Option<(Ident, TokenStream)>,
    verifier_error: Option<(Ident, TokenStream)>,

    is_serde: Option<Ident>,
    serde_spans: Option<(Ident, TokenStream)>,

    is_schemars: Option<Ident>,
}

impl VerifyOptions {
    fn merge(&mut self, other: VerifyOptions) {
        if let Some(v) = other.verifier {
            if let Some(existing_v) = &self.verifier {
                emit_error!(existing_v, "{} defined here", existing_v);
                abort!(v, r#"duplicate keys "{}""#, v);
            }

            self.verifier = v.into();
        }

        if let Some(v) = other.verifier_name {
            if let Some(existing_v) = &self.verifier_name {
                emit_error!(existing_v.0, "{} defined here", existing_v.0);
                abort!(v.0, r#"duplicate keys "{}""#, v.0);
            }

            self.verifier_name = v.into();
        }

        if let Some(v) = other.verifier_create {
            if let Some(existing_v) = &self.verifier_create {
                emit_error!(existing_v.0, "{} defined here", existing_v.0);
                abort!(v.0, r#"duplicate keys "{}""#, v.0);
            }

            self.verifier_create = v.into();
        }

        if let Some(v) = other.verifier_error {
            if let Some(existing_v) = &self.verifier_error {
                emit_error!(existing_v.0, "{} defined here", existing_v.0);
                abort!(v.0, r#"duplicate keys "{}""#, v.0);
            }

            self.verifier_error = v.into();
        }

        if let Some(v) = other.is_serde {
            if let Some(existing_v) = &self.is_serde {
                emit_error!(existing_v, "{} defined here", existing_v);
                abort!(v, r#"duplicate keys "{}""#, v);
            }

            self.is_serde = v.into();
        }

        if let Some(v) = other.serde_spans {
            if let Some(existing_v) = &self.serde_spans {
                emit_error!(existing_v.0, "{} defined here", existing_v.0);
                abort!(v.0, r#"duplicate keys "{}""#, v.0);
            }

            self.serde_spans = v.into();
        }

        if let Some(v) = other.is_schemars {
            if let Some(existing_v) = &self.is_schemars {
                emit_error!(existing_v, "{} defined here", existing_v);
                abort!(v, r#"duplicate keys "{}""#, v);
            }

            self.is_schemars = v.into();
        }
    }

    fn parse_serde_options(&mut self, content: ParseStream) -> syn::Result<()> {
        if content.is_empty() {
            return Ok(());
        }

        let serde_id: Ident = content.parse()?;

        if serde_id == "spans" {
            content.parse::<Token![=]>()?;
            let s = content.parse::<LitStr>()?;
            let ts: TokenStream = s.parse()?;
            self.serde_spans = Some((serde_id, ts));
        } else {
            abort!(serde_id, r#"unknown serde option "{}""#, serde_id);
        }

        Ok(())
    }

    fn parse_verifier_options(&mut self, content: ParseStream) -> syn::Result<()> {
        loop {
            if content.is_empty() {
                return Ok(());
            }

            let id: Ident = content.parse()?;

            if id == "name" {
                content.parse::<Token![=]>()?;
                let s = content.parse::<LitStr>()?;
                let ts: TokenStream = s.parse()?;
                self.verifier_name = Some((id, ts));
            } else if id == "error" {
                content.parse::<Token![=]>()?;
                let s = content.parse::<LitStr>()?;
                let ts: TokenStream = s.parse()?;
                self.verifier_error = Some((id, ts));
            } else if id == "create" {
                content.parse::<Token![=]>()?;
                let s = content.parse::<LitStr>()?;
                let ts: TokenStream = s.parse()?;
                self.verifier_create = Some((id, ts));
            } else {
                abort!(id, r#"unknown verifier option "{}""#, id);
            }

            if content.peek(Token![,]) {
                content.parse::<Token!(,)>()?;
            }
        }
    }

    fn parse_option(&mut self, content: ParseStream) -> syn::Result<()> {
        let id = content.parse::<Ident>()?;

        if id == "serde" {
            self.is_serde = Some(id);
            if content.peek(token::Paren) {
                let serde_content;
                parenthesized!(serde_content in content);
                self.parse_serde_options(&serde_content)?;
            }

            return Ok(());
        }

        if id == "verifier" {
            self.verifier = id.clone().into();
            if content.peek(token::Paren) {
                let verifier_content;
                parenthesized!(verifier_content in content);
                self.parse_verifier_options(&verifier_content)?;
            } else {
                content.parse::<Token![=]>()?;
                let s = content.parse::<LitStr>()?;
                let ts: TokenStream = s.parse()?;
                self.verifier_name = Some((id, ts));
            }

            return Ok(());
        }

        if id == "schemars" {
            self.is_schemars = Some(id);
            return Ok(());
        }

        abort!(id, r#"unknown option "{}""#, id);
    }
}

impl Parse for VerifyOptions {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut opts = VerifyOptions::default();

        if !input.peek(token::Paren) {
            return Ok(opts);
        }
        let content;
        parenthesized!(content in input);
        loop {
            if content.is_empty() {
                break;
            }

            opts.parse_option(&content)?;

            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }

        Ok(opts)
    }
}

struct Verify {
    input: syn::DeriveInput,
    options: VerifyOptions,
}

impl Verify {
    fn new(input: syn::DeriveInput, options: VerifyOptions) -> Self {
        Self { input, options }
    }

    fn check_options(&self) {
        if let Some(is_schemars) = &self.options.is_schemars {
            if self.options.is_serde.is_none() {
                abort!(
                    is_schemars,
                    r#"Serde is required for Schemars, use the "serde" option to enable it"#
                );
            }
        }
    }

    fn derive(self) -> TokenStream {
        self.check_options();

        if self.options.is_serde.is_some() {
            return self.derive_serde();
        }

        let ident = self.input.ident;
        let (impl_gen, ty_gen, where_gen) = self.input.generics.split_for_impl();

        let verifier_name = self
            .options
            .verifier_name
            .unwrap_or_else(|| abort_call_site!("verifier name is required"))
            .1;

        let verifier_error = match self.options.verifier_error {
            Some(e) => e.1,
            None => {
                quote! {
                    <#verifier_name as ::verify::Verifier<<Self as ::verify::span::Spanned>::Span>>::Error
                }
            }
        };

        let verifier_create = match self.options.verifier_create {
            Some(e) => e.1,
            None => {
                quote! {
                    #verifier_name::default()
                }
            }
        };

        quote! {
            impl#impl_gen ::verify::Verify for #ident#ty_gen #where_gen {
                type Error = #verifier_error;

                fn verify(&self) -> Result<(), Self::Error> {
                    let __v = #verifier_create;
                    <#verifier_name as ::verify::Verifier<<Self as ::verify::span::Spanned>::Span>>::verify_value(
                        &__v,
                        self,
                    )
                }
            }
        }
    }

    fn derive_serde(self) -> TokenStream {
        if self.options.is_schemars.is_some() {
            return self.derive_schemars();
        }

        let ident = self.input.ident;
        let (impl_gen, ty_gen, where_gen) = self.input.generics.split_for_impl();

        let spans = match self.options.serde_spans {
            Some((_, s)) => s,
            None => {
                quote! {::verify::serde::KeySpans}
            }
        };

        let verifier_name = self
            .options
            .verifier_name
            .unwrap_or_else(|| abort_call_site!("verifier name is required"))
            .1;

        let verifier_error = match self.options.verifier_error {
            Some(e) => e.1,
            None => {
                quote! {
                    <#verifier_name as ::verify::Verifier<<#spans as ::verify::serde::Spans>::Span>>::Error
                }
            }
        };

        let verifier_create = match self.options.verifier_create {
            Some(e) => e.1,
            None => {
                quote! {
                    #verifier_name::default()
                }
            }
        };

        quote! {
            impl#impl_gen ::verify::Verify for #ident#ty_gen #where_gen {
                type Error = #verifier_error;

                fn verify(&self) -> Result<(), Self::Error> {
                    let __v = #verifier_create;
                    <#verifier_name as ::verify::Verifier<<#spans as ::verify::serde::Spans>::Span>>::verify_value(
                        &__v,
                        &::verify::serde::Spanned::new(self, #spans::default()),
                    )
                }
            }
        }
    }

    fn derive_schemars(self) -> TokenStream {
        let ident = self.input.ident;
        let (impl_gen, ty_gen, where_gen) = self.input.generics.split_for_impl();

        let spans = match self.options.serde_spans {
            Some((_, s)) => s,
            None => {
                quote! {::verify::serde::KeySpans}
            }
        };

        if let Some(v) = self.options.verifier {
            abort!(v, "verifier option is not supported with Schemars");
        }

        if let Some(v) = self.options.verifier {
            abort!(v, "verifier option is not supported with Schemars");
        }

        let verifier_error = quote! {
            ::verify::schemars::errors::Errors<<#spans as ::verify::serde::Spans>::Span>
        };

        quote! {
            impl#impl_gen ::verify::Verify for #ident#ty_gen #where_gen {
                type Error = #verifier_error;

                fn verify(&self) -> Result<(), Self::Error> {
                    let __root = schemars::schema_for!(Self);

                    <schemars::schema::RootSchema as ::verify::Verifier<_>>::verify_value(
                        &__root,
                        &::verify::serde::Spanned::new(self, #spans::default()),
                    )
                }
            }
        }
    }
}
