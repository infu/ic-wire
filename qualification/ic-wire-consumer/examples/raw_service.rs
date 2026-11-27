//! Application-owned raw query and update. State lasts for this installation.
use core::cell::Cell;
use ic_platform_runtime::raw;
use wire::{Direction, Encoding, Limits};

#[path = "records/word.rs"]
mod word;
use word::Word;

thread_local! { static COUNT: Cell<u32> = const { Cell::new(0) }; }

fn respond(name: &str, handle: impl FnOnce(u32) -> u32) {
    const LIMITS: Limits = Limits {
        bytes: 11 + word::PAYLOAD_BYTES,
        allocation: 800,
        nodes: 16,
        depth: 4,
    };
    let mut input = [0; LIMITS.bytes];
    let Some(frame) = input.get_mut(..raw::msg_arg_data_size()) else {
        raw::trap(b"oversized frame")
    };
    raw::msg_arg_data_copy(frame, 0);
    let encoding = |direction| Encoding::Frame {
        method: wire::method_id(name),
        direction,
    };
    let request: Word = wire::decode(frame, encoding(Direction::Request), LIMITS)
        .unwrap_or_else(|e| raw::trap(e.to_string().as_bytes()));
    let mut output = [0; LIMITS.bytes];
    let reply = wire::encode_to_slice(
        &Word {
            value: handle(request.value),
        },
        encoding(Direction::Response),
        LIMITS,
        &mut output,
    )
    .unwrap_or_else(|e| raw::trap(e.to_string().as_bytes()));
    raw::msg_reply_data_append(reply);
    raw::msg_reply();
}

// read
// Return the current count plus the supplied offset, saturating at u32::MAX.
// limits : 17 request and reply bytes
#[expect(unsafe_code, reason = "Fixed IC query export")]
#[unsafe(export_name = "canister_query read")]
extern "C" fn read() {
    respond("read", |offset| COUNT.get().saturating_add(offset));
}

// add
// Add the supplied amount, saturating at u32::MAX, and return the new count.
// limits : 17 request and reply bytes
#[expect(unsafe_code, reason = "Fixed IC update export")]
#[unsafe(export_name = "canister_update add")]
extern "C" fn add() {
    respond("add", |amount| {
        let next = COUNT.get().saturating_add(amount);
        COUNT.set(next);
        next
    });
}
