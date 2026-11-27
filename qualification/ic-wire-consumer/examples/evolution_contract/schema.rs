//! Manual wire projections for the explicit Rust adapters in this fixture.
use crate::fixture::{Batch, Detail, Group, Row};
use wire::typescript::__private::{Value, json};
use wire::{
    BytesRef, Packed, TextRef,
    typescript::{GenerationError, SchemaContext, SchemaRegistry, ValidatedRegistry, Wire},
};

impl<const REV: u8> Wire for Batch<'_, Row<'_, REV>> {
    fn schema(ctx: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"extension-record","fields":[
            {"name":"groups","value":{"k":"list","max":4,"item":ctx.describe::<Group<Row<'_,REV>>>()?}},
            {"name":"suffix","value":ctx.describe::<TextRef<'_,32>>()?}
        ],"extensions":[]}))
    }
}

impl<const REV: u8> Wire for Group<Row<'_, REV>> {
    fn schema(ctx: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"record","fields":[
            {"name":"channel","value":ctx.describe::<u16>()?},
            {"name":"rows","value":{"k":"list","max":512,"item":ctx.describe::<Row<'_,REV>>()?}},
            {"name":"sequence","value":ctx.describe::<u64>()?}
        ]}))
    }
}

impl<const REV: u8> Wire for Row<'_, REV> {
    fn schema(ctx: &mut SchemaContext) -> Result<Value, GenerationError> {
        let mut states = vec![
            json!({"name":"Idle","id":1,"value":ctx.describe::<()>()?}),
            json!({"name":"Detail","id":2,"value":ctx.describe::<Detail<'_,REV>>()?}),
        ];
        let mut extensions = Vec::new();
        if REV == 2 {
            states
                .push(json!({"name":"Archived","id":3,"value":ctx.describe::<TextRef<'_,32>>()?}));
            extensions.push(
                json!({"name":"epoch","id":2,"required":true,"value":ctx.describe::<u32>()?}),
            );
        }
        if REV > 0 {
            extensions.push(json!({"name":"note","id":7,"required":false,"value":ctx.describe::<TextRef<'_,16>>()?}));
            extensions.push(
                json!({"name":"marker","id":9,"required":false,"value":ctx.describe::<()>()?}),
            );
        }
        Ok(json!({"k":"extension-record","fields":[
            {"name":"id","value":ctx.describe::<u32>()?},
            {"name":"state","value":{"k":"open-variant","variants":states}},
            {"name":"words","value":ctx.describe::<Packed<i16, 8>>()?}
        ],"extensions":extensions}))
    }
}

impl<const REV: u8> Wire for Detail<'_, REV> {
    fn schema(ctx: &mut SchemaContext) -> Result<Value, GenerationError> {
        let extensions = if REV == 2 {
            vec![
                json!({"name":"audit","id":4,"required":false,"value":ctx.describe::<BytesRef<'_,16>>()?}),
            ]
        } else {
            Vec::new()
        };
        Ok(json!({"k":"extension-record","fields":[
            {"name":"message","value":ctx.describe::<TextRef<'_,32>>()?}
        ],"extensions":extensions}))
    }
}

pub(super) fn graph<const REV: u8>() -> Result<ValidatedRegistry, GenerationError> {
    let mut registry = SchemaRegistry::new(Default::default())?;
    registry.register::<Batch<'_, Row<'_, REV>>>("Batch")?;
    registry.finish()
}
