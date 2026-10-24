//! Proc macros that declare the plugin API surface.
//!
//! [`vy_api_trait`] turns a host trait into method/event metadata and a
//! dispatch function. [`vy_api_method`] and [`vy_api_event`] mark the items it
//! collects. See `docs/adr/0013-host-api-traits.md`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Attribute, Expr, ExprLit, FnArg, GenericArgument, Ident, ItemTrait, Lit, LitStr, Meta,
    PathArguments, ReturnType, Token, TraitItem, Type,
};

/// Marks a trait method as one host method.
///
/// The enclosing [`vy_api_trait`] collects the method and removes this
/// attribute. The wire name defaults to the namespace path plus the camel-cased
/// method name; pass a string to override it. `manual` keeps a hand-written
/// wrapper in the shim.
#[proc_macro_attribute]
pub fn vy_api_method(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Marks a trait const as the declaration of a host event.
///
/// The enclosing [`vy_api_trait`] collects the declaration and removes this
/// attribute. The event is exposed as a subscription method named after the
/// last segment of the topic, which must start with `on`.
#[proc_macro_attribute]
pub fn vy_api_event(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Declares a host namespace as a trait and generates its metadata and dispatch.
///
/// Each trait method with `#[vy_api_method]` becomes a `MethodDecl` and an arm
/// in the generated `dispatch` function. Implementors only implement the trait.
#[proc_macro_attribute]
pub fn vy_api_trait(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand_trait(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[derive(Default)]
struct NamespaceArgs {
    path: Option<LitStr>,
}

impl Parse for NamespaceArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut args = Self::default();
        if input.is_empty() {
            return Ok(args);
        }
        if input.peek(LitStr) {
            args.path = Some(input.parse()?);
        }
        if input.peek(Token![,]) {
            return Err(input.error("unexpected namespace option"));
        }
        Ok(args)
    }
}

#[derive(Default)]
struct MethodArgs {
    name: Option<LitStr>,
    manual: bool,
    typed_key: Option<LitStr>,
}

impl Parse for MethodArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut args = Self::default();
        if input.is_empty() {
            return Ok(args);
        }
        if input.peek(LitStr) {
            args.name = Some(input.parse()?);
        }
        loop {
            if input.is_empty() {
                break;
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
                continue;
            }
            let key: Ident = input.parse()?;
            let key_name = key.to_string();
            match key_name.as_str() {
                "manual" => args.manual = true,
                "typed_key" => {
                    input.parse::<Token![=]>()?;
                    let name: Ident = input.parse()?;
                    args.typed_key = Some(LitStr::new(&name.to_string(), name.span()));
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown method option `{other}`"),
                    ));
                }
            }
        }
        Ok(args)
    }
}

struct EventArgs {
    topic: LitStr,
    payload: Type,
}

impl Parse for EventArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let topic: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let key: Ident = input.parse()?;
        if key != "payload" {
            return Err(syn::Error::new(
                key.span(),
                "expected `payload = Type` after the event topic",
            ));
        }
        input.parse::<Token![=]>()?;
        let payload: Type = input.parse()?;
        Ok(Self { topic, payload })
    }
}

fn expand_trait(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let args: NamespaceArgs = syn::parse2(attr)?;
    let path = args.path.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "`#[vy_api_trait]` needs a path, for example `#[vy_api_trait(\"fs\")]`",
        )
    })?;
    let namespace = path.value();
    let mut trait_item: ItemTrait = syn::parse2(item)?;
    let trait_doc = doc_of(&trait_item.attrs);
    let trait_ident = trait_item.ident.clone();

    let mut generated_items = Vec::new();
    let mut declarations = Vec::new();
    let mut event_entries = Vec::new();
    let mut arms = Vec::new();
    let mut retained = Vec::new();

    for item in std::mem::take(&mut trait_item.items) {
        let item = match item {
            TraitItem::Const(mut marker)
                if marker
                    .attrs
                    .iter()
                    .any(|attribute| is_attribute(attribute, "vy_api_event")) =>
            {
                let position = marker
                    .attrs
                    .iter()
                    .position(|attribute| is_attribute(attribute, "vy_api_event"))
                    .expect("checked above");
                let attribute = marker.attrs.remove(position);
                let event = parse_event_args(&attribute)?;
                let topic = event.topic.value();
                let (event_namespace, name) = topic.rsplit_once('.').ok_or_else(|| {
                    syn::Error::new_spanned(
                        &event.topic,
                        "an event topic must be `<namespace>.<name>`",
                    )
                })?;
                if !name.starts_with("on") {
                    return Err(syn::Error::new_spanned(
                        &event.topic,
                        "an event name must start with `on`",
                    ));
                }
                let identifier = marker.ident.clone();
                let doc = doc_of(&marker.attrs);
                let payload = event.payload;
                let marker = format_ident!("{}", pascal_case(name));
                let payload_fn = format_ident!("__vy_event_payload_{}", identifier);
                let payload_collect = format_ident!("__vy_event_collect_{}", identifier);
                let declaration = format_ident!("__VY_EVENT_{}", identifier);

                generated_items.push(quote! {
                    /// Marker for this event.
                    pub struct #marker;

                    impl ::vy_host_api::HostEvent for #marker {
                        const TOPIC: &'static str = #topic;
                        type Payload = #payload;
                    }

                    #[allow(non_snake_case)]
                    fn #payload_fn() -> String {
                        ::vy_api_schema::ts_name::<#payload>()
                    }

                    #[allow(non_snake_case)]
                    fn #payload_collect(collector: &mut ::vy_api_schema::TypeCollector) {
                        ::vy_api_schema::collect::<#payload>(collector);
                    }

                    static #declaration: ::vy_api_schema::EventDecl =
                        ::vy_api_schema::EventDecl {
                            topic: #topic,
                            name: #name,
                            namespace: #event_namespace,
                            payload: ::vy_api_schema::TypeRef {
                                name: #payload_fn as fn() -> String,
                                collect: #payload_collect
                                    as fn(&mut ::vy_api_schema::TypeCollector),
                            },
                            doc: #doc,
                        };
                });
                event_entries.push(quote! { #declaration });
                continue;
            }
            other => other,
        };
        let TraitItem::Fn(mut method) = item else {
            retained.push(item);
            continue;
        };
        let Some(position) = method
            .attrs
            .iter()
            .position(|attribute| is_attribute(attribute, "vy_api_method"))
        else {
            retained.push(TraitItem::Fn(method));
            continue;
        };
        let attribute = method.attrs.remove(position);
        let method_args = parse_method_args(&attribute)?;

        let mut inputs = method.sig.inputs.iter();
        match inputs.next() {
            Some(FnArg::Receiver(_)) => {}
            Some(other) => {
                return Err(syn::Error::new_spanned(
                    other,
                    "a host trait method needs `&mut self` as its first argument",
                ));
            }
            None => {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    "a host trait method needs `&mut self` as its first argument",
                ));
            }
        }
        let params = match inputs.next() {
            Some(FnArg::Typed(typed)) => Some((*typed.ty).clone()),
            None => None,
            Some(FnArg::Receiver(receiver)) => {
                return Err(syn::Error::new_spanned(
                    receiver,
                    "a host trait method takes at most `&mut self` and `params`",
                ));
            }
        };
        if let Some(extra) = inputs.next() {
            return Err(syn::Error::new_spanned(
                extra,
                "a host trait method takes at most `&mut self` and `params`",
            ));
        }

        let value = reply_value_type(&method.sig.output)?;
        let identifier = method.sig.ident.clone();
        let wire = method_args
            .name
            .map(|name| name.value())
            .unwrap_or_else(|| format!("{namespace}.{}", camel_case(&identifier.to_string())));
        let doc = doc_of(&method.attrs);
        let manual = method_args.manual;
        let typed_key = match &method_args.typed_key {
            Some(name) => quote! { Some(#name) },
            None => quote! { None },
        };

        let params_ts = format_ident!("__vy_params_ts_{}", identifier);
        let params_collect = format_ident!("__vy_params_collect_{}", identifier);
        let result_ts = format_ident!("__vy_result_ts_{}", identifier);
        let result_collect = format_ident!("__vy_result_collect_{}", identifier);
        let declaration = format_ident!("{}", identifier.to_string().to_uppercase());

        let (params_ts_item, params_field) = match &params {
            Some(params) => (
                quote! {
                    fn #params_ts() -> String {
                        ::vy_api_schema::ts_name::<#params>()
                    }

                    fn #params_collect(collector: &mut ::vy_api_schema::TypeCollector) {
                        ::vy_api_schema::collect::<#params>(collector);
                    }
                },
                quote! {
                    Some(::vy_api_schema::TypeRef {
                        name: #params_ts as fn() -> String,
                        collect: #params_collect
                            as fn(&mut ::vy_api_schema::TypeCollector),
                    })
                },
            ),
            None => (quote! {}, quote! { None }),
        };

        generated_items.push(quote! {
            #params_ts_item

            fn #result_ts() -> String {
                ::vy_api_schema::ts_name::<#value>()
            }

            fn #result_collect(collector: &mut ::vy_api_schema::TypeCollector) {
                ::vy_api_schema::collect::<#value>(collector);
            }

            pub static #declaration: ::vy_api_schema::MethodDecl =
                ::vy_api_schema::MethodDecl {
                    method: #wire,
                    params: #params_field,
                    result: ::vy_api_schema::TypeRef {
                        name: #result_ts as fn() -> String,
                        collect: #result_collect
                            as fn(&mut ::vy_api_schema::TypeCollector),
                    },
                    doc: #doc,
                    manual: #manual,
                    typed_key: #typed_key,
                };
        });
        declarations.push(quote! { #declaration });

        match &params {
            Some(params) => arms.push(quote! {
                #wire => Some((|| {
                    let params: #params = ::vy_host_api::parse_params(params)?;
                    ::vy_host_api::erase_reply(host.#identifier(params)?)
                })()),
            }),
            None => arms.push(quote! {
                #wire => Some((|| {
                    ::vy_host_api::erase_reply(host.#identifier()?)
                })()),
            }),
        }

        retained.push(TraitItem::Fn(method));
    }
    trait_item.items = retained;

    let generated = quote! {
        #(#generated_items)*

        /// Every method declared by this trait.
        pub static METHODS: &[::vy_api_schema::MethodDecl] = &[#(#declarations),*];

        /// Every event declared by this trait.
        pub static EVENTS: &[::vy_api_schema::EventDecl] = &[#(#event_entries),*];

        /// JSDoc for the namespace.
        pub static NAMESPACE: ::vy_api_schema::NamespaceDecl =
            ::vy_api_schema::NamespaceDecl {
                path: #namespace,
                doc: #trait_doc,
            };

        /// Answers a call declared by this trait.
        pub fn dispatch<H: #trait_ident + ?Sized>(
            host: &mut H,
            method: &str,
            params: &::serde_json::Value,
        ) -> Option<
            ::core::result::Result<
                ::vy_host_api::Reply<::serde_json::Value>,
                ::vy_host_api::CallError,
            >,
        > {
            match method {
                #(#arms)*
                _ => None,
            }
        }
    };

    Ok(quote! { #trait_item #generated })
}

fn is_attribute(attribute: &Attribute, name: &str) -> bool {
    attribute
        .path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == name)
}

fn parse_method_args(attribute: &Attribute) -> syn::Result<MethodArgs> {
    match &attribute.meta {
        Meta::Path(_) => Ok(MethodArgs::default()),
        Meta::List(list) => syn::parse2(list.tokens.clone()),
        Meta::NameValue(value) => Err(syn::Error::new_spanned(
            value,
            "use `#[vy_api_method]`, `#[vy_api_method(\"wire.name\")]`, or `#[vy_api_method(manual)]`",
        )),
    }
}

fn parse_event_args(attribute: &Attribute) -> syn::Result<EventArgs> {
    match &attribute.meta {
        Meta::List(list) => syn::parse2(list.tokens.clone()),
        _ => Err(syn::Error::new_spanned(
            attribute,
            "use `#[vy_api_event(\"topic\", payload = Type)]`",
        )),
    }
}

fn reply_value_type(output: &ReturnType) -> syn::Result<Type> {
    let error = || {
        syn::Error::new_spanned(
            output,
            "a handler must return `Result<Reply<T>, CallError>`",
        )
    };
    let ReturnType::Type(_, ty) = output else {
        return Err(error());
    };
    let Type::Path(path) = &**ty else {
        return Err(error());
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(error());
    };
    if segment.ident != "Result" {
        return Err(error());
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(error());
    };
    let Some(GenericArgument::Type(reply)) = arguments.args.first() else {
        return Err(error());
    };
    let Type::Path(reply) = reply else {
        return Err(error());
    };
    let Some(segment) = reply.path.segments.last() else {
        return Err(error());
    };
    if segment.ident != "Reply" {
        return Err(error());
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(error());
    };
    let Some(GenericArgument::Type(value)) = arguments.args.first() else {
        return Err(error());
    };
    Ok(value.clone())
}

fn doc_of(attributes: &[Attribute]) -> String {
    let mut lines = Vec::new();
    for attribute in attributes {
        if !is_attribute(attribute, "doc") {
            continue;
        }
        if let Meta::NameValue(value) = &attribute.meta
            && let Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) = &value.value
        {
            let text = text.value();
            lines.push(text.strip_prefix(' ').unwrap_or(&text).to_string());
        }
    }
    lines.join("\n")
}

fn pascal_case(name: &str) -> String {
    let mut characters = name.chars();
    match characters.next() {
        Some(first) => {
            let mut output = first.to_uppercase().collect::<String>();
            output.push_str(characters.as_str());
            output
        }
        None => String::new(),
    }
}

fn camel_case(name: &str) -> String {
    let mut output = String::new();
    for (index, part) in name.split('_').filter(|part| !part.is_empty()).enumerate() {
        if index == 0 {
            output.push_str(part);
        } else {
            let mut characters = part.chars();
            if let Some(first) = characters.next() {
                output.extend(first.to_uppercase());
                output.push_str(characters.as_str());
            }
        }
    }
    output
}
