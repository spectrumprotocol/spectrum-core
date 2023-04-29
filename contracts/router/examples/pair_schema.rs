use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

use spectrum::router::{ExecuteMsg, InstantiateMsg, QueryMsg, MAX_ASSETS, CallbackMsg, SwapOperation, SwapOperationRequest, Route};


fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(CallbackMsg), &out_dir);
    export_schema(&schema_for!(SwapOperation), &out_dir);
    export_schema(&schema_for!(SwapOperationRequest), &out_dir);
    export_schema(&schema_for!(Route), &out_dir);
}
