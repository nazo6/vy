//! Proc macros that declare the plugin API surface.
//!
//! [`vy_api_namespace`] wraps an inline module of handler functions, and
//! [`vy_api_registry`] joins those modules into the registry the host and the
//! codegen read. See `docs/adr/0008-declare-plugin-api-with-macros.md`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Attribute, Expr, ExprLit, FnArg, GenericArgument, Ident, Item, ItemFn, ItemMod, Lit, LitStr,
    Meta, PathArguments, ReturnType, Token, Type,
};

/// Marks a handler function as one host method.
///
/// The enclosing [`vy_api_namespace`] collects the function and removes this
/// attribute. The wire name defaults to the namespace path plus the camel-cased
/// function name; pass a string to override it. `manual` keeps a hand-written
/// wrapper in the shim.
#[proc_macro_attribute]
pub fn vy_api_method(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Marks a const as the declaration of a host event.
///
/// The enclosing [`vy_api_namespace`] collects the declaration and removes this
/// attribute. The event is exposed as a subscription method named after the
/// last segment of the topic, which must start with `on`.
#[proc_macro_attribute]
pub fn vy_api_event(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Declares an inline module as one namespace of the plugin API.
///
/// The argument is the dotted namespace path. A doc comment on the module
/// becomes the JSDoc of the namespace; doc comments on the handler functions
/// become the JSDoc of the methods. `sugar = Name` names the hand-written
/// TypeScript interface that the generated namespace extends.
#[proc_macro_attribute]
pub fn vy_api_namespace(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand_namespace(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Builds the registry that joins the namespace modules.
///
/// Apply it to an inline module whose only items are `mod name;` declarations.
/// The generated `methods`, `dispatch`, and `namespace_decls` read each module's
/// `METHODS`, `dispatch`, and `NAMESPACE`.
#[proc_macro_attribute]
pub fn vy_api_registry(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand_registry(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[derive(Default)]
struct NamespaceArgs {
    path: Option<LitStr>,
    sugar: Option<LitStr>,
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
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "sugar" => {
                    let name: Ident = input.parse()?;
                    args.sugar = Some(LitStr::new(&name.to_string(), name.span()));
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown namespace option `{other}`"),
                    ));
                }
            }
        }
        Ok(args)
    }
}

#[derive(Default)]
struct MethodArgs {
    name: Option<LitStr>,
    manual: bool,
    typed_key: Option<LitStr>,
    typed_args: Option<LitStr>,
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
                "typed_key" | "typed_args" => {
                    input.parse::<Token![=]>()?;
                    let name: Ident = input.parse()?;
                    let value = LitStr::new(&name.to_string(), name.span());
                    if key_name == "typed_key" {
                        args.typed_key = Some(value);
                    } else {
                        args.typed_args = Some(value);
                    }
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

fn expand_namespace(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let args: NamespaceArgs = syn::parse2(attr)?;
    let path = args.path.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "`#[vy_api_namespace]` needs a path, for example `#[vy_api_namespace(\"editor\")]`",
        )
    })?;
    let namespace = path.value();

    let mut module: ItemMod = syn::parse2(item)?;
    let namespace_doc = doc_of(&module.attrs);
    let Some((_, items)) = module.content.as_mut() else {
        return Err(syn::Error::new_spanned(
            &module,
            "`#[vy_api_namespace]` requires an inline module (`mod name { ... }`); an out-of-line `mod name;` cannot be inspected",
        ));
    };

    let mut generated_items = Vec::new();
    let mut entries = Vec::new();
    let mut arms = Vec::new();
    let mut event_entries = Vec::new();
    let mut retained = Vec::new();

    for item in std::mem::take(items) {
        match item {
            Item::Fn(mut function) => {
                let Some(position) = function
                    .attrs
                    .iter()
                    .position(|attribute| is_attribute(attribute, "vy_api_method"))
                else {
                    retained.push(Item::Fn(function));
                    continue;
                };
                let attribute = function.attrs.remove(position);
                let method = parse_method_args(&attribute)?;

                let call_arg = function.sig.inputs.first().ok_or_else(|| {
                    syn::Error::new_spanned(
                        &function.sig,
                        "a handler needs `call: &mut Call<'_>` as its first argument",
                    )
                })?;
                match call_arg {
                    FnArg::Typed(typed) if is_call(&typed.ty) => {}
                    _ => {
                        return Err(syn::Error::new_spanned(
                            call_arg,
                            "the first argument must be `call: &mut Call<'_>`",
                        ));
                    }
                }

                let params = params_type(&function)?;
                let value = reply_value_type(&function.sig.output)?;
                let identifier = function.sig.ident.clone();
                let wire = method.name.map(|name| name.value()).unwrap_or_else(|| {
                    format!("{namespace}.{}", camel_case(&identifier.to_string()))
                });
                let doc = doc_of(&function.attrs);
                let manual = method.manual;
                let typed_key = match &method.typed_key {
                    Some(name) => quote! { Some(#name) },
                    None => quote! { None },
                };
                let typed_args = match &method.typed_args {
                    Some(name) => quote! { Some(#name) },
                    None => quote! { None },
                };

                let adapter = format_ident!("__vy_invoke_{}", identifier);
                let params_ts = format_ident!("__vy_params_ts_{}", identifier);
                let result_ts = format_ident!("__vy_result_ts_{}", identifier);
                let declaration =
                    format_ident!("__VY_METHOD_{}", identifier.to_string().to_uppercase());

                let (params_ts_item, params_field, parse_params, invoke) = match &params {
                    Some(params) => (
                        quote! {
                            fn #params_ts() -> String {
                                ::vy_api_schema::ts_name::<#params>()
                            }
                        },
                        quote! { Some(#params_ts as fn() -> String) },
                        quote! { let params: #params = crate::api::parse_params(raw)?; },
                        quote! { #identifier(call, params) },
                    ),
                    None => (
                        quote! {},
                        quote! { None },
                        quote! { let _ = raw; },
                        quote! { #identifier(call) },
                    ),
                };

                generated_items.push(quote! {
                    #params_ts_item

                    fn #result_ts() -> String {
                        ::vy_api_schema::ts_name::<#value>()
                    }

                    fn #adapter(
                        call: &mut crate::api::Call<'_, '_>,
                        raw: &::serde_json::Value,
                    ) -> ::core::result::Result<
                        crate::api::Reply<::serde_json::Value>,
                        crate::api::CallError,
                    > {
                        #parse_params
                        let reply = #invoke?;
                        crate::api::erase_reply(reply)
                    }

                    static #declaration: ::vy_api_schema::MethodDecl =
                        ::vy_api_schema::MethodDecl {
                            method: #wire,
                            params: #params_field,
                            result: #result_ts as fn() -> String,
                            doc: #doc,
                            manual: #manual,
                            typed_key: #typed_key,
                            typed_args: #typed_args,
                        };
                });
                entries.push(quote! {
                    crate::api::HandlerDecl {
                        decl: &#declaration,
                        handler: #adapter,
                    }
                });
                arms.push(quote! {
                    #wire => Some(#adapter(call, params)),
                });
                retained.push(Item::Fn(function));
            }
            Item::Const(mut marker) => {
                let Some(position) = marker
                    .attrs
                    .iter()
                    .position(|attribute| is_attribute(attribute, "vy_api_event"))
                else {
                    retained.push(Item::Const(marker));
                    continue;
                };
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
                let payload_fn = format_ident!("__vy_event_payload_{}", identifier);
                let declaration = format_ident!("__VY_EVENT_{}", identifier);
                let topic_const = format_ident!("{}_TOPIC", identifier);

                generated_items.push(quote! {
                    #[allow(non_snake_case)]
                    fn #payload_fn() -> String {
                        ::vy_api_schema::ts_name::<#payload>()
                    }

                    static #declaration: ::vy_api_schema::EventDecl =
                        ::vy_api_schema::EventDecl {
                            topic: #topic,
                            name: #name,
                            namespace: #event_namespace,
                            payload: #payload_fn as fn() -> String,
                            doc: #doc,
                        };

                    pub const #topic_const: &str = #topic;
                });
                event_entries.push(quote! { #declaration });
                // The marker const exists only to attach the attribute.
            }
            other => retained.push(other),
        }
    }
    *items = retained;

    let sugar = match &args.sugar {
        Some(name) => quote! { Some(#name) },
        None => quote! { None },
    };

    let generated = quote! {
        #(#generated_items)*

        /// Every method declared by this namespace.
        pub static METHODS: &[crate::api::HandlerDecl] = &[#(#entries),*];

        /// Every event declared by this namespace.
        pub static EVENTS: &[::vy_api_schema::EventDecl] = &[#(#event_entries),*];

        /// JSDoc and sugar metadata for this namespace.
        pub static NAMESPACE: ::vy_api_schema::NamespaceDecl =
            ::vy_api_schema::NamespaceDecl {
                path: #namespace,
                doc: #namespace_doc,
                sugar: #sugar,
            };

        /// Answers a call declared by this namespace.
        pub fn dispatch(
            call: &mut crate::api::Call<'_, '_>,
            method: &str,
            params: &::serde_json::Value,
        ) -> Option<
            ::core::result::Result<
                crate::api::Reply<::serde_json::Value>,
                crate::api::CallError,
            >,
        > {
            match method {
                #(#arms)*
                _ => None,
            }
        }
    };

    let generated: syn::File = syn::parse2(generated)?;
    items.extend(generated.items);
    Ok(quote! { #module })
}

fn expand_registry(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    if !attr.is_empty() {
        return Err(syn::Error::new_spanned(
            &attr,
            "`#[vy_api_registry]` takes no arguments",
        ));
    }
    let mut module: ItemMod = syn::parse2(item)?;
    let Some((_, items)) = module.content.as_mut() else {
        return Err(syn::Error::new_spanned(
            &module,
            "`#[vy_api_registry]` requires an inline module",
        ));
    };

    let mut names = Vec::new();
    for item in items.iter() {
        if let Item::Mod(child) = item
            && child.content.is_none()
        {
            names.push(child.ident.clone());
        }
    }
    if names.is_empty() {
        return Err(syn::Error::new_spanned(
            &module,
            "`#[vy_api_registry]` needs at least one `mod name;`",
        ));
    }

    let generated = quote! {
        /// Every declared method, across namespaces.
        pub fn methods() -> Vec<&'static crate::api::HandlerDecl> {
            let mut methods = Vec::new();
            #(methods.extend(#names::METHODS);)*
            methods
        }

        /// Every declared event, across namespaces.
        pub fn events() -> Vec<&'static ::vy_api_schema::EventDecl> {
            let mut events = Vec::new();
            #(events.extend(#names::EVENTS);)*
            events
        }

        /// Answers a call from any namespace.
        pub fn dispatch(
            call: &mut crate::api::Call<'_, '_>,
            method: &str,
            params: &::serde_json::Value,
        ) -> Option<
            ::core::result::Result<
                crate::api::Reply<::serde_json::Value>,
                crate::api::CallError,
            >,
        > {
            #(
                if let Some(result) = #names::dispatch(call, method, params) {
                    return Some(result);
                }
            )*
            None
        }

        /// JSDoc and sugar metadata for every namespace.
        pub fn namespace_decls() -> Vec<&'static ::vy_api_schema::NamespaceDecl> {
            let mut namespaces = Vec::new();
            #(namespaces.push(&#names::NAMESPACE);)*
            namespaces
        }
    };

    let generated: syn::File = syn::parse2(generated)?;
    items.extend(generated.items);
    Ok(quote! { #module })
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

fn is_call(ty: &Type) -> bool {
    let Type::Reference(reference) = ty else {
        return false;
    };
    let Type::Path(path) = &*reference.elem else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Call")
}

fn params_type(function: &ItemFn) -> syn::Result<Option<Type>> {
    let mut inputs = function.sig.inputs.iter();
    inputs.next().ok_or_else(|| {
        syn::Error::new_spanned(&function.sig, "a handler needs `call: &mut Call<'_>`")
    })?;
    let params = match inputs.next() {
        None => None,
        Some(FnArg::Typed(typed)) => Some((*typed.ty).clone()),
        Some(FnArg::Receiver(receiver)) => {
            return Err(syn::Error::new_spanned(
                receiver,
                "a handler cannot take `self`",
            ));
        }
    };
    if let Some(extra) = inputs.next() {
        return Err(syn::Error::new_spanned(
            extra,
            "a handler takes at most `call` and `params`",
        ));
    }
    Ok(params)
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
