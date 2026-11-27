//! Inline logical record declarations. The generated transport proxies use Serde.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::BTreeSet;
use syn::{
    Data, DeriveInput, Field, Fields, GenericParam, Ident, Index, Lifetime, LifetimeParam, LitInt,
    LitStr, Member, Path, Type,
};

struct Slot<'a> {
    member: Member,
    name: String,
    ty: &'a Type,
}

struct Addition<'a> {
    id: u32,
    slot: Slot<'a>,
    required: bool,
}

struct Layout<'a> {
    tuple: bool,
    core: Vec<Slot<'a>>,
    extensions: Vec<Addition<'a>>,
}

#[derive(Default)]
struct FieldOptions {
    id: Option<u32>,
    rename: Option<String>,
    required: bool,
}

pub(crate) fn expand(input: &DeriveInput, owner: &Path) -> syn::Result<TokenStream> {
    let layout = layout(input)?;
    let codec = codec(input, owner, &layout);
    let schema = schema(input, owner, &layout);
    Ok(quote! {
        #codec
        #owner::__wire_evolution_schema! { #schema }
    })
}

fn layout(input: &DeriveInput) -> syn::Result<Layout<'_>> {
    let Data::Struct(record) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "inline evolution requires a struct",
        ));
    };
    let tuple = matches!(record.fields, Fields::Unnamed(_));
    let mut core = Vec::new();
    let mut extensions = Vec::new();
    let mut names = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for (position, field) in record.fields.iter().enumerate() {
        let FieldOptions {
            id,
            rename,
            required,
        } = field_attributes(field)?;
        let member = field
            .ident
            .clone()
            .map_or_else(|| Member::Unnamed(Index::from(position)), Member::Named);
        let name = match (&field.ident, rename) {
            (None, Some(_)) => {
                return Err(syn::Error::new_spanned(
                    field,
                    "tuple slot names follow their positions",
                ));
            }
            (None, None) => format!("slot{}", position + 1),
            (Some(_), Some(name)) => name,
            (Some(ident), None) => {
                let name = ident.to_string();
                name.strip_prefix("r#").unwrap_or(&name).to_owned()
            }
        };
        if name.is_empty() || !names.insert(name.clone()) {
            return Err(syn::Error::new_spanned(
                field,
                "record schema names must be unique and nonempty",
            ));
        }
        if let Some(id) = id {
            if id == 0 || !ids.insert(id) {
                return Err(syn::Error::new_spanned(
                    field,
                    "extension IDs must be positive and unique",
                ));
            }
            if tuple && u32::try_from(position + 1) != Ok(id) {
                return Err(syn::Error::new_spanned(
                    field,
                    "tuple extension IDs are one-based positions",
                ));
            }
            let optional = super::optional_content(&field.ty);
            let ty = if required {
                if tuple || optional.is_some() {
                    return Err(syn::Error::new_spanned(
                        field,
                        "wire(required) requires a non-optional named extension field",
                    ));
                }
                &field.ty
            } else {
                optional.ok_or_else(|| {
                    syn::Error::new_spanned(
                        field,
                        "wire(id = N) in an inline record requires Option<T> or wire(required)",
                    )
                })?
            };
            extensions.push(Addition {
                id,
                slot: Slot { member, name, ty },
                required,
            });
        } else {
            if required {
                return Err(syn::Error::new_spanned(
                    field,
                    "wire(required) needs wire(id = N)",
                ));
            }
            if tuple && !extensions.is_empty() {
                return Err(syn::Error::new_spanned(
                    field,
                    "fixed tuple slots must precede optional extensions",
                ));
            }
            core.push(Slot {
                member,
                name,
                ty: &field.ty,
            });
        }
    }
    extensions.sort_unstable_by_key(|addition| addition.id);
    Ok(Layout {
        tuple,
        core,
        extensions,
    })
}

fn field_attributes(field: &Field) -> syn::Result<FieldOptions> {
    let (rename, _) = crate::member_attributes(
        field
            .attrs
            .iter()
            .filter(|attr| !attr.path().is_ident("wire")),
        true,
        None,
    )?;
    let mut options = FieldOptions {
        rename,
        ..FieldOptions::default()
    };
    for attr in field
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("wire"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("id") && options.id.is_none() {
                let value: LitInt = meta.value()?.parse()?;
                options.id = Some(value.base10_parse()?);
            } else if meta.path.is_ident("rename") {
                if options.rename.is_some() {
                    return Err(meta.error("field already has a Serde or Wire rename"));
                }
                let value: LitStr = meta.value()?.parse()?;
                options.rename = Some(value.value());
            } else if meta.path.is_ident("required") && !options.required {
                options.required = true;
            } else {
                return Err(
                    meta.error("expected wire(id = N), wire(required), or wire(rename = \"name\")")
                );
            }
            Ok(())
        })?;
    }
    Ok(options)
}

fn schema(input: &DeriveInput, owner: &Path, layout: &Layout<'_>) -> TokenStream {
    let name = &input.ident;
    let context = crate::local_name(input, "__wire_context");
    let (kind, entries) = if layout.tuple {
        ("extension-tuple", "items")
    } else {
        ("extension-record", "fields")
    };
    let mut generics = input.generics.clone();
    for slot in layout
        .core
        .iter()
        .chain(layout.extensions.iter().map(|addition| &addition.slot))
    {
        let ty = slot.ty;
        generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#ty: #owner::typescript::Wire));
    }
    let (implementation, parameters, bounds) = generics.split_for_impl();
    let core = layout.core.iter().map(|slot| {
        let Slot { name, ty, .. } = slot;
        if layout.tuple {
            quote!(#context.describe::<#ty>()?)
        } else {
            quote!({"name":#name,"value":#context.describe::<#ty>()?})
        }
    });
    let extensions = layout.extensions.iter().map(|addition| {
        let Addition { id, slot, required } = addition;
        let Slot { name, ty, .. } = slot;
        quote!({"id":#id,"name":#name,"value":#context.describe::<#ty>()?,"required":#required})
    });
    quote! {
        const _: () = {
            impl #implementation #owner::evolution::__private::EvolvingSchema for #name #parameters #bounds {
                fn describe(#context: &mut #owner::typescript::SchemaContext) -> ::core::result::Result<#owner::typescript::__private::Value, #owner::typescript::GenerationError> {
                    <Self as #owner::typescript::Wire>::schema(#context)
                }
            }

            impl #implementation #owner::typescript::Wire for #name #parameters #bounds {
                fn schema(#context: &mut #owner::typescript::SchemaContext) -> ::core::result::Result<#owner::typescript::__private::Value, #owner::typescript::GenerationError> {
                ::core::result::Result::Ok(#owner::typescript::__private::json!({
                        "k":#kind,#entries:[#(#core),*],"extensions":[#(#extensions),*],"projected":true
                }))
                }
            }
        };
    }
}

struct Proxies {
    encoder: Ident,
    decoder: Ident,
    visitor: Ident,
    serde: TokenStream,
    slots: Vec<Ident>,
    indices: Vec<Index>,
    extension: Index,
    count: usize,
}

impl Proxies {
    fn new(input: &DeriveInput, owner: &Path, core_count: usize) -> Self {
        let type_prefix = crate::local_name(input, "__WireCore");
        Self {
            encoder: crate::local_name(input, "__WireRecordWriter"),
            decoder: crate::local_name(input, "__WireRecordReader"),
            visitor: crate::local_name(input, "__WireRecordVisitor"),
            serde: quote!(#owner::evolution::__serde),
            slots: (0..core_count)
                .map(|position| format_ident!("{type_prefix}{position}"))
                .collect(),
            indices: (0..core_count).map(Index::from).collect(),
            extension: Index::from(core_count),
            count: core_count + 1,
        }
    }

    fn writer(&self, owner: &Path) -> TokenStream {
        let Self {
            encoder,
            serde,
            slots,
            indices,
            extension,
            count,
            ..
        } = self;
        quote! {
            pub struct #encoder<'wire, #(#slots,)*>(#(#slots,)* #owner::evolution::Extensions<'wire>);

            impl<'wire, #(#slots: #serde::Serialize,)*> #serde::Serialize for #encoder<'wire, #(#slots,)*> {
                fn serialize<S: #serde::Serializer>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error> {
                    let mut fields = serializer.serialize_tuple(#count)?;
                    #(#serde::ser::SerializeTuple::serialize_element(&mut fields, &self.#indices)?;)*
                    #serde::ser::SerializeTuple::serialize_element(&mut fields, &self.#extension)?;
                    #serde::ser::SerializeTuple::end(fields)
                }
            }
        }
    }

    fn reader(&self, owner: &Path) -> TokenStream {
        let Self {
            decoder,
            visitor,
            serde,
            slots,
            indices,
            count,
            ..
        } = self;
        let ordinary = indices.iter().map(|index| index.index as usize);
        let extensions = self.extension.index as usize;
        quote! {
            pub struct #decoder<'wire, #(#slots,)*>(#(#slots,)* #owner::evolution::Extensions<'wire>);
            struct #visitor<'wire, #(#slots,)*>(::core::marker::PhantomData<#decoder<'wire, #(#slots,)*>>);

            impl<'wire, #(#slots: #serde::Deserialize<'wire>,)*> #serde::de::Visitor<'wire>
                for #visitor<'wire, #(#slots,)*>
            {
                type Value = #decoder<'wire, #(#slots,)*>;

                fn expecting(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    formatter.write_str("fixed fields followed by a sparse extension block")
                }

                fn visit_seq<A: #serde::de::SeqAccess<'wire>>(self, mut fields: A) -> ::core::result::Result<Self::Value, A::Error> {
                    ::core::result::Result::Ok(#decoder(
                        #(fields.next_element()?.ok_or_else(|| #serde::de::Error::invalid_length(#ordinary, &self))?,)*
                        fields.next_element()?.ok_or_else(|| #serde::de::Error::invalid_length(#extensions, &self))?,
                    ))
                }
            }

            impl<'wire, #(#slots: #serde::Deserialize<'wire>,)*> #serde::Deserialize<'wire>
                for #decoder<'wire, #(#slots,)*>
            {
                fn deserialize<D: #serde::Deserializer<'wire>>(deserializer: D) -> ::core::result::Result<Self, D::Error> {
                    deserializer.deserialize_tuple(#count, #visitor(::core::marker::PhantomData))
                }
            }
        }
    }
}

fn codec(input: &DeriveInput, owner: &Path, layout: &Layout<'_>) -> TokenStream {
    let name = &input.ident;
    let local = |base| crate::local_name(input, base);
    let de = Lifetime::new(&format!("'{}", local("__wire_read")), Span::call_site());
    let stage = Lifetime::new(&format!("'{}", local("__wire_stage")), Span::call_site());
    let inline = quote!(#owner::evolution::inline);
    let projection = quote!(#inline::Inline<#de>);
    let input_bytes = local("__wire_input");
    let output = local("__wire_output");
    let encoding = local("__wire_encoding");
    let limits = local("__wire_limits");
    let scratch = local("__wire_scratch");
    let context = local("__wire_context");
    let writer = local("__wire_writer");
    let block = local("__wire_block");
    let dto = local("__wire_dto");
    let field_id = local("__wire_id");
    let field = local("__wire_field");
    let value = local("__wire_value");
    let incompatible = local("__wire_incompatible");
    let mut generics = input.generics.clone();
    generics
        .params
        .insert(0, GenericParam::Lifetime(LifetimeParam::new(de.clone())));
    for slot in layout
        .core
        .iter()
        .chain(layout.extensions.iter().map(|addition| &addition.slot))
    {
        let ty = slot.ty;
        generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#ty: #projection));
    }
    let (implementation, _, bounds) = generics.split_for_impl();
    let (_, parameters, _) = input.generics.split_for_impl();

    let proxies = Proxies::new(input, owner, layout.core.len());
    let Proxies {
        encoder,
        decoder,
        indices,
        extension,
        ..
    } = &proxies;
    let declares_extensions = !layout.extensions.is_empty();
    let stages_core = layout.core.iter().map(|slot| {
        let ty = slot.ty;
        quote!(<#ty as #projection>::MAY_STAGE)
    });
    let encoded_types = layout.core.iter().map(|slot| {
        let ty = slot.ty;
        quote!(<#ty as #projection>::Encoded<#stage>)
    });
    let decoded_types = layout.core.iter().map(|slot| {
        let ty = slot.ty;
        quote!(<#ty as #projection>::Decoded)
    });
    let core_prefix = local("__wire_core");
    let core_values: Vec<_> = (0..layout.core.len())
        .map(|position| format_ident!("{core_prefix}_{position}"))
        .collect();
    let block_type = quote!(#owner::evolution::Extensions);
    // Serde provides tuple implementations up to 16 elements, including the extension block.
    let (proxy_types, encoded_type, decoded_type, encoded_dto) = if proxies.count <= 16 {
        (
            TokenStream::new(),
            quote!((#(#encoded_types,)* #block_type<#stage>,)),
            quote!((#(#decoded_types,)* #block_type<#de>,)),
            quote!((#(#core_values,)* #block,)),
        )
    } else {
        let encoder_impl = proxies.writer(owner);
        let decoder_impl = proxies.reader(owner);
        (
            quote!(#encoder_impl #decoder_impl),
            quote!(#encoder<#stage, #(#encoded_types,)*>),
            quote!(#decoder<#de, #(#decoded_types,)*>),
            quote!(#encoder(#(#core_values,)* #block)),
        )
    };
    // A public Inline impl cannot expose a private field type through its
    // associated types. A nominal transparent view keeps those types private.
    let (public_views, encoded_type, decoded_type, encoded_dto, unwrap_dto) = if matches!(
        input.vis,
        syn::Visibility::Inherited
    ) {
        (
            TokenStream::new(),
            encoded_type,
            decoded_type,
            encoded_dto,
            TokenStream::new(),
        )
    } else {
        let encoded_view = local("__WireEncodedView");
        let decoded_view = local("__WireDecodedView");
        let serializer = local("__WireSerializer");
        let deserializer = local("__WireDeserializer");
        let serializer_input = local("__wire_serializer_input");
        let deserializer_input = local("__wire_deserializer_input");
        let serde = quote!(#owner::evolution::__serde);
        let (_, decoded_parameters, _) = generics.split_for_impl();
        let mut encoded_generics = generics.clone();
        encoded_generics
            .params
            .insert(0, GenericParam::Lifetime(LifetimeParam::new(stage.clone())));
        encoded_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#name #parameters: #stage));
        let (encoded_impl, encoded_parameters, encoded_bounds) = encoded_generics.split_for_impl();
        let public_views = quote! {
            #[repr(transparent)]
            pub struct #encoded_view #encoded_impl (
                #encoded_type,
                ::core::marker::PhantomData<(&#stage #name #parameters, &#de ())>,
            ) #encoded_bounds;

            impl #encoded_impl #serde::Serialize for #encoded_view #encoded_parameters #encoded_bounds {
                #[inline]
                fn serialize<#serializer: #serde::Serializer>(
                    &self,
                    #serializer_input: #serializer,
                ) -> ::core::result::Result<#serializer::Ok, #serializer::Error> {
                    #serde::Serialize::serialize(&self.0, #serializer_input)
                }
            }

            #[repr(transparent)]
            pub struct #decoded_view #implementation (
                #decoded_type,
                ::core::marker::PhantomData<fn() -> #name #parameters>,
            ) #bounds;

            impl #implementation #serde::Deserialize<#de> for #decoded_view #decoded_parameters #bounds {
                #[inline]
                fn deserialize<#deserializer: #serde::Deserializer<#de>>(
                    #deserializer_input: #deserializer,
                ) -> ::core::result::Result<Self, #deserializer::Error> {
                    <#decoded_type as #serde::Deserialize<#de>>::deserialize(#deserializer_input)
                        .map(|#value| Self(#value, ::core::marker::PhantomData))
                }
            }
        };
        (
            public_views,
            quote!(#encoded_view #encoded_parameters),
            quote!(#decoded_view #decoded_parameters),
            quote!(#encoded_view(#encoded_dto, ::core::marker::PhantomData)),
            quote!(let #dto = #dto.0;),
        )
    };
    let prepare_core = layout.core.iter().zip(&core_values).map(|(slot, binding)| {
        let Slot { member, ty, .. } = slot;
        quote! {
            let (#binding, #scratch) =
                <#ty as #projection>::prepare(&self.#member, &#context.child(), #scratch)?;
        }
    });
    let project_core =
        layout
            .core
            .iter()
            .zip(&core_values)
            .zip(indices)
            .map(|((slot, binding), index)| {
                let ty = slot.ty;
                quote! {
                    let #binding = #inline::preserve_incompatible(
                        <#ty as #projection>::project(#dto.#index, &#context.child()),
                        &mut #incompatible,
                    )?;
                }
            });
    let writer_mut = (!layout.extensions.is_empty()).then(|| quote!(mut));
    let local_prefix = local("__wire_member");
    let additions: Vec<_> = (0..layout.extensions.len())
        .map(|position| format_ident!("{local_prefix}_{position}"))
        .collect();
    let pushes = layout.extensions.iter().map(|addition| {
        let Addition { id, slot, required } = addition;
        let member = &slot.member;
        let ty = slot.ty;
        if *required {
            quote!(#writer.push_projected::<#ty>(#context, #id, &self.#member)?;)
        } else {
            quote! {
                if let ::core::option::Option::Some(#value) = &self.#member {
                    #writer.push_projected::<#ty>(#context, #id, #value)?;
                }
            }
        }
    });
    let assignment = layout
        .extensions
        .iter()
        .zip(&additions)
        .map(|(addition, binding)| {
            let Addition { id, slot, .. } = addition;
            let ty = slot.ty;
            quote! {
                #id => {
                    #binding = #inline::preserve_incompatible(
                        #field.project_inline::<#ty>(),
                        &mut #incompatible,
                    )?;
                }
            }
        });
    let core_members = layout.core.iter().zip(&core_values).map(|(slot, binding)| {
        let member = &slot.member;
        quote!(#member: #binding.ok_or(#owner::Error::Incompatible)?)
    });
    let extension_members = layout
        .extensions
        .iter()
        .zip(&additions)
        .map(|(addition, binding)| {
            let member = &addition.slot.member;
            if addition.required {
                quote!(#member: #binding.ok_or(#owner::Error::Incompatible)?)
            } else {
                quote!(#member: #binding)
            }
        });
    let (declare_incompatible, check_incompatible) =
        if layout.core.is_empty() && layout.extensions.is_empty() {
            (TokenStream::new(), TokenStream::new())
        } else {
            (
                quote!(let mut #incompatible = false;),
                quote! {
                    if #incompatible {
                        return ::core::result::Result::Err(#owner::Error::Incompatible);
                    }
                },
            )
        };
    let unit = matches!(&input.data, Data::Struct(data) if matches!(data.fields, Fields::Unit));
    let construct = if unit {
        quote!(Self)
    } else {
        quote!(Self {
            #(#core_members,)*
            #(#extension_members,)*
        })
    };
    quote! {
        const _: () = {
            #proxy_types
            #public_views

            impl #implementation #projection for #name #parameters #bounds {
                const MAY_STAGE: bool = #declares_extensions #(|| #stages_core)*;

                type Encoded<#stage> = #encoded_type where Self: #stage;
                type Decoded = #decoded_type;

                fn prepare<#stage>(
                    &#stage self,
                    #context: &#owner::evolution::WriteContext<'_>,
                    #scratch: &#stage mut [u8],
                ) -> ::core::result::Result<(Self::Encoded<#stage>, &#stage mut [u8]), #owner::Error> {
                    #context.admit_preparation()?;
                    #(#prepare_core)*
                    let #writer_mut #writer = #owner::evolution::ExtensionsWriter::new(#scratch);
                    #(#pushes)*
                    let (#block, #scratch) = #writer.finish_and_tail();
                    ::core::result::Result::Ok((#encoded_dto, #scratch))
                }

                fn project(
                    #dto: Self::Decoded,
                    #context: &#owner::evolution::ReadContext<'_>,
                ) -> ::core::result::Result<Self, #owner::Error> {
                    #unwrap_dto
                    #declare_incompatible
                    #(#project_core)*
                    #(let mut #additions = ::core::option::Option::None;)*
                    #dto.#extension.visit(#context, |#field_id, #field| {
                        match #field_id {
                            #(#assignment,)*
                            _ => {}
                        }
                        ::core::result::Result::Ok(())
                    })?;
                    #check_incompatible
                    ::core::result::Result::Ok(#construct)
                }
            }

            impl #implementation #owner::evolution::Evolving<#de> for #name #parameters #bounds {
                fn encode_evolving(
                    &self,
                    #output: &mut #owner::evolution::__private::Vec<u8>,
                    #encoding: #owner::Encoding,
                    #limits: #owner::Limits,
                ) -> ::core::result::Result<(), #owner::Error> {
                    #inline::encode(self, #output, #encoding, #limits)
                }

                fn decode_evolving(
                    #input_bytes: &#de [u8],
                    #encoding: #owner::Encoding,
                    #limits: #owner::Limits,
                ) -> ::core::result::Result<Self, #owner::Error> {
                    #inline::decode(#input_bytes, #encoding, #limits)
                }
            }
        };
    }
}
