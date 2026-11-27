//! Declarations for typed envelopes. Leaf encoding stays with Serde.

mod record;
pub(super) use record::expand as expand_record;

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::BTreeSet;
use syn::{
    Attribute, Data, DeriveInput, Fields, GenericArgument, Ident, Lifetime, LitInt, LitStr, Path,
    PathArguments, Type,
};

struct Member {
    id: u32,
    name: String,
    required: bool,
    key: Ident,
    value: Type,
    unit_case: bool,
}

pub(super) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let (owner, _) = super::type_options(input, None)?;
    let (members, is_open) = collect(input)?;
    let schema = schema(input, &owner, &members, is_open);
    let codec = if is_open {
        open_codec(input, &owner, &members)
    } else {
        extension_codec(input, &owner, &members)
    };
    Ok(quote! {
        #owner::__wire_evolution_schema! { #schema }
        #codec
    })
}

fn collect(input: &DeriveInput) -> syn::Result<(Vec<Member>, bool)> {
    let mut members = Vec::new();
    let is_open = match &input.data {
        Data::Struct(record) => {
            let Fields::Named(fields) = &record.fields else {
                return Err(syn::Error::new_spanned(
                    &input.ident,
                    "extension declarations require named fields",
                ));
            };
            for field in &fields.named {
                let key = field.ident.as_ref().unwrap();
                let (id, name, required) = identity(key, &field.attrs, true)?;
                let value = if required {
                    &field.ty
                } else {
                    optional_content(&field.ty).ok_or_else(|| {
                        syn::Error::new_spanned(
                            &field.ty,
                            "optional extension fields require Option<T>; use wire(required) for T",
                        )
                    })?
                };
                members.push(Member {
                    id,
                    name,
                    required,
                    key: key.clone(),
                    value: value.clone(),
                    unit_case: false,
                });
            }
            false
        }
        Data::Enum(choice) if !choice.variants.is_empty() => {
            for variant in &choice.variants {
                let key = &variant.ident;
                let (id, name, required) = identity(key, &variant.attrs, false)?;
                if variant.discriminant.is_some() || id > u32::MAX >> 1 {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "open cases use wire(id = N) in 1..=2147483647",
                    ));
                }
                let (value, unit_case) = match &variant.fields {
                    Fields::Unit => (syn::parse_quote!(()), true),
                    Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                        let field = &fields.unnamed[0];
                        super::reject_attributes(&field.attrs)?;
                        (field.ty.clone(), false)
                    }
                    _ => {
                        return Err(syn::Error::new_spanned(
                            variant,
                            "open cases must be unit or carry one Serde payload",
                        ));
                    }
                };
                members.push(Member {
                    id,
                    name,
                    required,
                    key: key.clone(),
                    value,
                    unit_case,
                });
            }
            true
        }
        _ => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "expected a named extension struct or a nonempty open enum",
            ));
        }
    };
    let (mut ids, mut names) = (BTreeSet::new(), BTreeSet::new());
    for member in &members {
        if !ids.insert(member.id) || !names.insert(&member.name) {
            return Err(syn::Error::new_spanned(
                &member.key,
                "evolution IDs and schema names must each be unique",
            ));
        }
    }
    members.sort_unstable_by_key(|member| member.id);
    Ok((members, is_open))
}

fn identity(key: &Ident, attrs: &[Attribute], extension: bool) -> syn::Result<(u32, String, bool)> {
    let mut id = None;
    let mut name = None;
    let mut required = false;
    for attr in attrs {
        if attr.path().is_ident("wire") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("id") && id.is_none() {
                    let value: LitInt = meta.value()?.parse()?;
                    id = Some(value.base10_parse::<u32>()?);
                } else if meta.path.is_ident("rename") && name.is_none() {
                    let value: LitStr = meta.value()?.parse()?;
                    name = Some(value.value());
                } else if extension && meta.path.is_ident("required") && !required {
                    required = true;
                } else {
                    return Err(meta.error("unsupported or duplicate evolution attribute"));
                }
                Ok(())
            })?;
        } else {
            super::reject_attributes(std::slice::from_ref(attr))?;
        }
    }
    let id = id
        .filter(|id| *id > 0)
        .ok_or_else(|| syn::Error::new_spanned(key, "expected a positive wire(id = N)"))?;
    let name = name.unwrap_or_else(|| {
        let name = key.to_string();
        name.strip_prefix("r#").unwrap_or(&name).to_owned()
    });
    if name.is_empty() {
        return Err(syn::Error::new_spanned(
            key,
            "evolution schema names must be nonempty",
        ));
    }
    Ok((id, name, required))
}

fn optional_content(ty: &Type) -> Option<&Type> {
    let Type::Path(ty) = ty else { return None };
    let segments = &ty.path.segments;
    let names = segments
        .iter()
        .map(|segment| &segment.ident)
        .collect::<Vec<_>>();
    if ty.qself.is_some()
        || !matches!(names.as_slice(), [name] if *name == "Option")
            && !matches!(names.as_slice(), [root, module, name]
                if (*root == "core" || *root == "std") && *module == "option" && *name == "Option")
    {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segments.last()?.arguments else {
        return None;
    };
    let GenericArgument::Type(item) = args.args.first()? else {
        return None;
    };
    (args.args.len() == 1).then_some(item)
}

fn schema(input: &DeriveInput, owner: &Path, members: &[Member], is_open: bool) -> TokenStream {
    let name = &input.ident;
    let context = super::local_name(input, "__wire_context");
    let mut generics = input.generics.clone();
    for ty in generics.type_params_mut() {
        ty.bounds.push(syn::parse_quote!(#owner::typescript::Wire));
    }
    let (implementation, parameters, bounds) = generics.split_for_impl();
    let (layout, method) = if is_open {
        (format_ident!("OpenLayout"), format_ident!("open_cases"))
    } else {
        (
            format_ident!("ExtensionLayout"),
            format_ident!("extension_fields"),
        )
    };
    let fields = members.iter().map(|member| {
        let Member {
            id,
            name,
            required,
            value,
            ..
        } = member;
        let required = (!is_open).then(|| quote!(,"required":#required));
        quote!({"id":#id,"name":#name,"value":#context.describe::<#value>()? #required})
    });
    quote! {
        impl #implementation #owner::evolution::#layout for #name #parameters #bounds {
            fn #method(#context: &mut #owner::typescript::SchemaContext) -> ::core::result::Result<#owner::typescript::__private::Value, #owner::typescript::GenerationError> {
                ::core::result::Result::Ok(#owner::typescript::__private::json!([#(#fields),*]))
            }
        }
    }
}

struct CodecLocals {
    context: Ident,
    scratch: Ident,
    data: Ident,
    id: Ident,
    value: Ident,
    input: Lifetime,
    output: Lifetime,
}

impl CodecLocals {
    fn new(input: &DeriveInput) -> Self {
        let local = |base| super::local_name(input, base);
        let lifetime = |base| Lifetime::new(&format!("'{}", local(base)), Span::call_site());
        Self {
            context: local("__wire_context"),
            scratch: local("__wire_scratch"),
            data: local("__wire_data"),
            id: local("__wire_id"),
            value: local("__wire_value"),
            input: lifetime("__wire_input"),
            output: lifetime("__wire_output"),
        }
    }

    fn bounds(&self, owner: &Path, members: &[Member]) -> (TokenStream, TokenStream) {
        let needs = members
            .iter()
            .filter(|member| !member.unit_case)
            .map(|member| &member.value);
        let read = needs.clone();
        let write = needs.clone();
        let de = &self.input;
        if needs.count() == 0 {
            return (TokenStream::new(), TokenStream::new());
        }
        (
            quote!(where #(#write: #owner::evolution::__serde::Serialize,)*),
            quote!(where #(#read: #owner::evolution::__serde::Deserialize<#de>,)*),
        )
    }
}

fn extension_codec(input: &DeriveInput, owner: &Path, members: &[Member]) -> TokenStream {
    let name = &input.ident;
    let vis = &input.vis;
    let (implementation, parameters, bounds) = input.generics.split_for_impl();
    let locals = CodecLocals::new(input);
    let (write, read) = locals.bounds(owner, members);
    let CodecLocals {
        context,
        scratch,
        data,
        id: field_id,
        value,
        input: de,
        output: out,
    } = locals;
    let decoder = super::local_name(input, "__wire_decode_extensions");
    let member_prefix = super::local_name(input, "__wire_member");
    let projectors = super::local_name(input, "__wire_projectors");
    let projector_prefix = super::local_name(input, "__wire_project");
    let bindings: Vec<_> = (0..members.len())
        .map(|index| format_ident!("{member_prefix}_{index}"))
        .collect();
    let projects: Vec<_> = (0..members.len())
        .map(|index| format_ident!("{projector_prefix}_{index}"))
        .collect();
    let keys = members.iter().map(|member| &member.key);
    let types: Vec<_> = members.iter().map(|member| &member.value).collect();
    let push = members.iter().map(|member| {
        let Member {
            id, key, required, ..
        } = member;
        if *required {
            quote!(#data.push(#context, #id, &self.#key)?;)
        } else {
            quote!(
                if let ::core::option::Option::Some(#value) = &self.#key {
                    #data.push(#context, #id, #value)?;
                }
            )
        }
    });
    let assignments =
        members
            .iter()
            .zip(&bindings)
            .zip(&projects)
            .map(|((member, binding), project)| {
                let id = member.id;
                quote!(#id => { #binding = ::core::option::Option::Some(#project(#value)?); })
            });
    let bind_projects =
        (!projects.is_empty()).then(|| quote!(let (#(mut #projects,)*) = #projectors;));
    let required = members
        .iter()
        .zip(&bindings)
        .filter(|(member, _)| member.required)
        .map(|(_, binding)| {
            quote! {
                let ::core::option::Option::Some(#binding) = #binding else {
                    return ::core::result::Result::Ok(::core::option::Option::None);
                };
            }
        });
    quote! {
        impl #implementation #name #parameters #bounds {
            /// Encode the declared extensions in ascending ID order into caller scratch.
            /// # Errors
            /// Returns the first resource, encoding or scratch refusal.
            #[allow(dead_code, reason = "A consuming binary may only use the reader")]
            #vis fn encode_extensions<#out>(
                &self,
                #context: &#owner::evolution::WriteContext<'_>,
                #scratch: &#out mut [u8],
            ) -> ::core::result::Result<#owner::evolution::ExtensionSlot<#out, Self>, #owner::Error>
            #write
            {
                let mut #data = #owner::evolution::ExtensionsWriter::new(#scratch);
                #(#push)*
                ::core::result::Result::Ok(#owner::evolution::ExtensionSlot::from_raw(#data.finish()))
            }

            /// Decode this reader's extensions, skipping unknown IDs and checking exact extents.
            /// # Errors
            /// Returns structural or resource refusals, or Incompatible for missing required IDs.
            #[allow(dead_code, reason = "A consuming binary may only use the writer")]
            #vis fn decode_extensions<#de>(
                #data: #owner::evolution::ExtensionSlot<#de, Self>,
                #context: &#owner::evolution::ReadContext<'_>,
            ) -> ::core::result::Result<Self, #owner::Error>
            #read
            {
                Self::#decoder(#data, #context, (#(|field| field.decode::<#types>(),)*))?
                    .ok_or(#owner::Error::Incompatible)
            }

            /// Decode extensions and mark a missing required ID in the active recovery scope.
            ///
            /// Supply one projector for each declared field in ascending ID order.
            /// Present fields are decoded and projected at their own admitted depth before a
            /// missing ID can recover. For a Serde leaf, an identity projector is sufficient.
            /// Fields containing evolution envelopes must project all known descendants and
            /// propagate failures before returning their validated value.
            /// # Errors
            /// Returns structural, known value or resource failures after checking the block.
            #[allow(dead_code, reason = "A consuming binary may only use strict projection")]
            #vis fn decode_extensions_in<#de>(
                #data: #owner::evolution::ExtensionSlot<#de, Self>,
                #context: &#owner::evolution::RecoveryContext<'_>,
                #projectors: (#(impl FnMut(#types, &#owner::evolution::RecoveryContext<'_>) -> ::core::result::Result<#types, #owner::Error>,)*),
            ) -> ::core::result::Result<::core::option::Option<Self>, #owner::Error>
            #read
            {
                #bind_projects
                let #value = Self::#decoder(
                    #data,
                    #context.read_context(),
                    (#(|field| #context.project_field(field, &mut #projects),)*),
                )?;
                if #value.is_none() {
                    #context.missing_required();
                }
                ::core::result::Result::Ok(#value)
            }

            fn #decoder<#de>(
                #data: #owner::evolution::ExtensionSlot<#de, Self>,
                #context: &#owner::evolution::ReadContext<'_>,
                #projectors: (#(impl FnMut(#owner::evolution::Field<'_, #de>) -> ::core::result::Result<#types, #owner::Error>,)*),
            ) -> ::core::result::Result<::core::option::Option<Self>, #owner::Error>
            {
                #bind_projects
                #(let mut #bindings = ::core::option::Option::None;)*
                #data.into_raw().visit(#context, |#field_id, #value| {
                    match #field_id {
                        #(#assignments,)*
                        _ => {}
                    }
                    ::core::result::Result::Ok(())
                })?;
                #(#required)*
                ::core::result::Result::Ok(::core::option::Option::Some(Self { #(#keys: #bindings),* }))
            }
        }
    }
}

fn open_codec(input: &DeriveInput, owner: &Path, members: &[Member]) -> TokenStream {
    let name = &input.ident;
    let vis = &input.vis;
    let (implementation, parameters, bounds) = input.generics.split_for_impl();
    let locals = CodecLocals::new(input);
    let (write, read) = locals.bounds(owner, members);
    let CodecLocals {
        context,
        scratch,
        data,
        id: case_id,
        value,
        input: de,
        output: out,
    } = locals;
    let case_decoder = super::local_name(input, "__wire_decode_case");
    let encode_cases = members.iter().map(|member| {
        let Member { key, id, unit_case, .. } = member;
        if *unit_case {
            quote!(Self::#key => #owner::evolution::OpaqueOpen::unit(#id)?,)
        } else {
            quote!(Self::#key(#value) => #owner::evolution::OpaqueOpen::write(#context, #id, #value, #scratch)?,)
        }
    });
    let decode_cases = members.iter().map(|member| {
        let Member {
            key,
            id,
            value: ty,
            unit_case,
            ..
        } = member;
        let result = if *unit_case {
            quote!({ #value.decode::<()>()?; Self::#key })
        } else {
            quote!(Self::#key(#value.decode::<#ty>()?))
        };
        quote!(#id => ::core::option::Option::Some(#result),)
    });
    quote! {
        impl #implementation #name #parameters #bounds {
            /// Encode an absent or known open case into caller scratch.
            /// # Errors
            /// Returns the first resource, encoding or scratch refusal.
            #[allow(dead_code, reason = "A consuming binary may only use the reader")]
            #vis fn encode_open<#out>(
                #data: ::core::option::Option<&Self>,
                #context: &#owner::evolution::WriteContext<'_>,
                #scratch: &#out mut [u8],
            ) -> ::core::result::Result<#owner::evolution::OpenSlot<#out, Self>, #owner::Error>
            #write
            {
                let #value = match #data {
                    ::core::option::Option::Some(#value) => #value.encode_required(#context, #scratch)?.into_raw(),
                    ::core::option::Option::None => #owner::evolution::OpaqueOpen::absent(),
                };
                ::core::result::Result::Ok(#owner::evolution::OpenSlot::from_raw(#value))
            }

            /// Encode a present required open case into caller scratch.
            /// # Errors
            /// Returns the first resource, encoding or scratch refusal.
            #[allow(dead_code, reason = "A consuming binary may only use optional cases or the reader")]
            #vis fn encode_required<#out>(
                &self,
                #context: &#owner::evolution::WriteContext<'_>,
                #scratch: &#out mut [u8],
            ) -> ::core::result::Result<#owner::evolution::OpenSlot<#out, Self, true>, #owner::Error>
            #write
            {
                let #value = match self { #(#encode_cases)* };
                ::core::result::Result::Ok(#owner::evolution::OpenSlot::from_raw(#value))
            }

            /// Decode a known open case, mapping an unfamiliar case to absence.
            /// # Errors
            /// Returns an error for malformed known values, invalid envelopes or resource refusal.
            #[allow(dead_code, reason = "A consuming binary may only use the writer")]
            #vis fn decode_open<#de>(
                #data: #owner::evolution::OpenSlot<#de, Self>,
                #context: &#owner::evolution::ReadContext<'_>,
            ) -> ::core::result::Result<::core::option::Option<Self>, #owner::Error>
            #read
            {
                #data.into_raw().visit(#context, Self::#case_decoder).map(::core::option::Option::flatten)
            }

            /// Decode a required open case and mark an unfamiliar ID in the active recovery scope.
            /// # Errors
            /// Returns an error for malformed known values, zero tags or resource refusal.
            #[allow(dead_code, reason = "A consuming binary may only use optional cases or the writer")]
            #vis fn decode_required<#de>(
                #data: #owner::evolution::OpenSlot<#de, Self, true>,
                #context: &#owner::evolution::RecoveryContext<'_>,
            ) -> ::core::result::Result<::core::option::Option<Self>, #owner::Error>
            #read
            {
                #context.required_open(#data.into_raw(), Self::#case_decoder)
            }

            fn #case_decoder<#de>(
                #case_id: u32,
                #value: #owner::evolution::Field<'_, #de>,
            ) -> ::core::result::Result<::core::option::Option<Self>, #owner::Error>
            #read
            {
                ::core::result::Result::Ok(match #case_id {
                    #(#decode_cases)*
                    _ => ::core::option::Option::None
                })
            }
        }
    }
}
