#![no_main]

use libfuzzer_sys::fuzz_target;
use std::convert::TryInto;
use sv1_api::{json_rpc::Message, methods::Method};

fuzz_target!(|data: &[u8]| {
    if let Ok(message) = serde_json::from_slice::<Message>(data) {
        let _method: Result<Method, _> = message.clone().try_into();

        if let Ok(serialized) = serde_json::to_vec(&message) {
            let _ = serde_json::from_slice::<Message>(&serialized);
        }
    }
});
