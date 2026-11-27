//! Projection of published Platform Types, which owns identity and Serde.
use super::{GenerationError, SchemaContext, Wire};
use serde_json::{Value, json};

impl Wire for ic_platform_types::Principal {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        context.describe::<crate::Bytes<{ Self::MAX_LENGTH_IN_BYTES }>>()
    }
}

impl Wire for ic_platform_types::Account {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"record","fixed":true,"fields":[
            {"name":"owner","value":context.describe::<ic_platform_types::Principal>()?},
            {"name":"subaccount","value":context.describe::<Option<crate::ByteArray<32>>>()?}
        ]}))
    }
}
