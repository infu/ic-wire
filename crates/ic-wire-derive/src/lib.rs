//! Wire declarations using stock Serde for payload fields.
mod evolve;
#[cfg(test)]
mod tests;

use proc_macro::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// Derive an evolvable Wire record or tuple from a struct.
///
/// Untagged fields form the positional prefix. Optional additions use
/// `#[wire(id = N)]`. A named addition may use `#[wire(id = N, required)]`
/// to require its presence. Tuple IDs are one-based optional slot positions.
/// Every evolving struct has one trailing extension block. This derive does
/// not implement or change the type's Serde traits.
/// On an evolving struct, symmetric Serde field names also name Wire fields.
/// Serde crate paths and field borrow declarations may accompany the derive.
///
/// Use `#[wire(fixed)]` to describe an ordinary fixed Serde struct. Enums
/// describe their closed, externally tagged Serde layout. These projections
/// support symmetric Serde renames, borrows and standard byte/packed adapters.
/// Use `#[wire(crate = "renamed_dependency")]` when ic-wire is renamed.
/// Use `#[wire(extensions)]` on a final `ExtensionSlot` field to describe a
/// record or tuple with explicit projection.
#[proc_macro_derive(Wire, attributes(serde, wire))]
pub fn derive_wire(input: TokenStream) -> TokenStream {
    expand(&parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Generate typed preparation, projection and schema for stable evolution IDs.
///
/// On a named struct, every field has `#[wire(id = N)]` and the value is
/// `Option<T>`. Use `#[wire(id = N, required)]` for a required `T`.
/// An enum declares open cases using `#[wire(id = N)]` on each unit or newtype
/// variant. The generated methods use Wire's admitted Postcard codec.
///
#[proc_macro_derive(Evolve, attributes(serde, wire))]
pub fn derive_evolve(input: TokenStream) -> TokenStream {
    evolve::expand(&parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn reject_attributes(attrs: &[syn::Attribute]) -> syn::Result<()> {
    for attr in attrs {
        if attr.path().is_ident("serde") || attr.path().is_ident("wire") {
            return Err(syn::Error::new_spanned(
                attr,
                "unsupported metadata attribute; supply an explicitly qualified manual projection",
            ));
        }
    }
    Ok(())
}

fn fields(
    fields: &Fields,
    owner: &syn::Path,
    context: &syn::Ident,
    allow_extensions: bool,
    fixed: bool,
) -> syn::Result<proc_macro2::TokenStream> {
    if matches!(fields, Fields::Unit) {
        return Ok(quote!(#owner::typescript::__private::json!({"k":"unit"})));
    }
    let named = matches!(fields, Fields::Named(_));
    let mut entries = Vec::new();
    let mut extension = None;
    for (index, field) in fields.iter().enumerate() {
        let (ordinary, carrier) = extension_attribute(&field.attrs)?;
        let (rename, adapter) = member_attributes(&ordinary, true, Some(owner))?;
        if carrier {
            if fixed {
                return Err(syn::Error::new_spanned(
                    field,
                    "wire(fixed) conflicts with wire(extensions)",
                ));
            }
            if !allow_extensions
                || index + 1 != fields.len()
                || rename.is_some()
                || adapter.is_some()
            {
                return Err(syn::Error::new_spanned(
                    field,
                    "wire(extensions) requires the final struct field with no adapter or rename",
                ));
            }
            extension = Some(&field.ty);
        } else {
            let value = projection(&field.ty, adapter, context);
            if let Some(ident) = &field.ident {
                let name = ident.to_string();
                let name =
                    rename.unwrap_or_else(|| name.strip_prefix("r#").unwrap_or(&name).to_owned());
                entries.push(quote!({"name":#name,"value":#value}));
            } else if rename.is_some() {
                return Err(syn::Error::new_spanned(
                    field,
                    "renaming an unnamed field is not supported",
                ));
            } else {
                entries.push(value);
            }
        }
    }
    let fixed = fixed.then(|| quote!(,"fixed":true));
    Ok(match (named, extension, entries.as_slice()) {
        (true, Some(ty), _) => quote!(#owner::typescript::__private::json!({
            "k":"extension-record","fields":[#(#entries),*],
            "extensions":<#ty as #owner::evolution::SchemaFields>::extension_fields(#context)?
        })),
        (false, Some(ty), _) => quote!(#owner::typescript::__private::json!({
            "k":"extension-tuple","items":[#(#entries),*],
            "extensions":<#ty as #owner::evolution::SchemaFields>::extension_fields(#context)?
        })),
        (true, None, _) => {
            quote!(#owner::typescript::__private::json!({"k":"record","fields":[#(#entries),*]#fixed}))
        }
        (false, None, [item]) => {
            quote!(#owner::typescript::__private::json!({"k":"newtype","item":#item}))
        }
        (false, None, _) => {
            quote!(#owner::typescript::__private::json!({"k":"tuple","items":[#(#entries),*]#fixed}))
        }
    })
}

fn extension_attribute(attrs: &[syn::Attribute]) -> syn::Result<(Vec<syn::Attribute>, bool)> {
    let mut ordinary = Vec::new();
    let mut carrier = false;
    for attr in attrs {
        if attr.path().is_ident("wire") {
            if carrier {
                return Err(syn::Error::new_spanned(
                    attr,
                    "expected one wire(extensions)",
                ));
            }
            attr.parse_nested_meta(|meta| {
                if !meta.path.is_ident("extensions") || carrier {
                    return Err(meta.error("expected one wire(extensions)"));
                }
                carrier = true;
                Ok(())
            })?;
            if !carrier {
                return Err(syn::Error::new_spanned(attr, "expected wire(extensions)"));
            }
        } else {
            ordinary.push(attr.clone());
        }
    }
    Ok((ordinary, carrier))
}

fn has_extension_carrier(fields: &Fields) -> syn::Result<bool> {
    use syn::{Meta, Token, punctuated::Punctuated};
    for attribute in fields
        .iter()
        .flat_map(|field| &field.attrs)
        .filter(|attribute| attribute.path().is_ident("wire"))
    {
        let entries = attribute.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
        if entries
            .iter()
            .any(|entry| entry.path().is_ident("extensions"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn type_options(input: &DeriveInput, marker: Option<&str>) -> syn::Result<(syn::Path, bool)> {
    let mut owner: syn::Path = syn::parse_quote!(::ic_wire);
    let mut renamed = false;
    let mut marked = false;
    let mut serde_crate = false;
    for attr in &input.attrs {
        if attr.path().is_ident("wire") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("crate") && !renamed {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    owner = value.parse()?;
                    renamed = true;
                    return Ok(());
                }
                if !marked && marker.is_some_and(|marker| meta.path.is_ident(marker)) {
                    marked = true;
                    return Ok(());
                }
                Err(match marker {
                    Some(marker) => meta.error(format_args!(
                        "expected wire(crate = \"dependency\") or wire({marker}) once"
                    )),
                    None => meta.error("expected one wire(crate = \"dependency\")"),
                })
            })?;
        } else if attr.path().is_ident("serde") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("crate") && !serde_crate {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    let _: syn::Path = value.parse()?;
                    serde_crate = true;
                    Ok(())
                } else {
                    Err(meta.error("unsupported or duplicate Serde type attribute"))
                }
            })?;
        }
    }
    Ok((owner, marked))
}

fn local_name(input: &DeriveInput, base: &str) -> syn::Ident {
    let source = input.to_token_stream().to_string();
    let mut local = base.to_owned();
    while source.contains(&local) {
        local.push('_');
    }
    format_ident!("{local}")
}

fn expand(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let (owner, fixed) = type_options(input, Some("fixed"))?;
    let explicit_extensions = match &input.data {
        Data::Struct(data) => has_extension_carrier(&data.fields)?,
        _ => false,
    };
    if matches!(input.data, Data::Struct(_)) && !fixed && !explicit_extensions {
        return evolve::expand_record(input, &owner);
    }
    let context = local_name(input, "__wire_context");
    let body = match &input.data {
        Data::Struct(data) => fields(&data.fields, &owner, &context, true, fixed)?,
        Data::Enum(data) => {
            let mut variants = Vec::new();
            for (id, variant) in data.variants.iter().enumerate() {
                let (renamed_variant, _) = member_attributes(&variant.attrs, false, None)?;
                if variant.discriminant.is_some() {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "explicit discriminants are not qualified",
                    ));
                }
                let name = variant.ident.to_string();
                let name = renamed_variant
                    .unwrap_or_else(|| name.strip_prefix("r#").unwrap_or(&name).to_owned());
                let value = if let Fields::Unnamed(unnamed) = &variant.fields {
                    if unnamed.unnamed.len() == 1 {
                        let bytes = unnamed_attributes(&unnamed.unnamed[0].attrs, &owner)?;
                        projection(&unnamed.unnamed[0].ty, bytes, &context)
                    } else {
                        fields(&variant.fields, &owner, &context, false, fixed)?
                    }
                } else {
                    fields(&variant.fields, &owner, &context, false, fixed)?
                };
                variants.push(quote!({"name":#name,"id":#id,"value":#value}));
            }
            quote!(#owner::typescript::__private::json!({"k":"enum","variants":[#(#variants),*]}))
        }
        Data::Union(data) => {
            return Err(syn::Error::new_spanned(
                data.union_token,
                "unions cannot be projected",
            ));
        }
    };
    let name = &input.ident;
    let fixed_marker = (!explicit_extensions).then(|| {
        let (implementation, parameters, bounds) = input.generics.split_for_impl();
        quote!(impl #implementation #owner::evolution::inline::Fixed for #name #parameters #bounds {})
    });
    let mut generics = input.generics.clone();
    for parameter in generics.type_params_mut() {
        parameter
            .bounds
            .push(syn::parse_quote!(#owner::typescript::Wire));
    }
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        #fixed_marker
        #owner::__wire_evolution_schema! {
            impl #impl_generics #owner::typescript::Wire for #name #type_generics #where_clause {
                fn schema(#context: &mut #owner::typescript::SchemaContext) -> ::core::result::Result<#owner::typescript::__private::Value, #owner::typescript::GenerationError> {
                    ::core::result::Result::Ok(#body)
                }
            }
        }
    })
}

#[derive(Clone, Copy)]
enum Adapter {
    Bytes,
    Packed(u8),
}

fn projection(
    ty: &syn::Type,
    bytes: Option<Adapter>,
    context: &syn::Ident,
) -> proc_macro2::TokenStream {
    match bytes {
        Some(Adapter::Bytes) => quote!(#context.describe_bytes::<#ty>()?),
        Some(Adapter::Packed(kind)) => quote!(#context.describe_packed::<#ty,#kind>()?),
        None => quote!(#context.describe::<#ty>()?),
    }
}

fn member_attributes<'a>(
    attrs: impl IntoIterator<Item = &'a syn::Attribute>,
    allow_field: bool,
    adapter_owner: Option<&syn::Path>,
) -> syn::Result<(Option<String>, Option<Adapter>)> {
    let mut borrowed = false;
    let mut renamed = None;
    let mut bytes = None;
    for attr in attrs {
        if attr.path().is_ident("serde") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") && renamed.is_none() {
                    let name: syn::LitStr = meta.value()?.parse()?;
                    renamed = Some(name.value());
                } else if meta.path.is_ident("with") && allow_field && bytes.is_none() {
                    let Some(owner) = adapter_owner else {
                        return Err(meta.error(
                            "express Serde adapters on evolving records with a fixed Wire field type",
                        ));
                    };
                    let path: syn::LitStr = meta.value()?.parse()?;
                    let path = path.value();
                    let path = path.trim_start_matches("::");
                    if path == "serde_bytes" {
                        bytes = Some(Adapter::Bytes);
                    } else {
                        let owner = owner.to_token_stream().to_string().replace(' ', "");
                        let prefix = format!("{}::serde_packed::", owner.trim_start_matches("::"));
                        let kind =
                            match path.strip_prefix(&prefix) {
                                Some("bools") => 0,
                                Some("finite_f32") => 1,
                                Some("finite_f64") => 2,
                                Some("nonzero_i64") => 3,
                                Some("heap_u64") => 4,
                                _ => return Err(meta.error(
                                    "only qualified standard byte/packed adapters are supported",
                                )),
                            };
                        bytes = Some(Adapter::Packed(kind));
                    }
                } else if meta.path.is_ident("borrow") && allow_field && !borrowed {
                    borrowed = true;
                    if meta.input.peek(syn::Token![=]) {
                        let _: syn::LitStr = meta.value()?.parse()?;
                    }
                } else {
                    return Err(meta.error("unsupported or duplicate Serde metadata attribute"));
                }
                Ok(())
            })?;
        } else {
            reject_attributes(std::slice::from_ref(attr))?;
        }
    }
    Ok((renamed, bytes))
}

fn unnamed_attributes(attrs: &[syn::Attribute], owner: &syn::Path) -> syn::Result<Option<Adapter>> {
    let (rename, bytes) = member_attributes(attrs, true, Some(owner))?;
    if rename.is_some() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "renaming an unnamed field is not supported",
        ));
    }
    Ok(bytes)
}
