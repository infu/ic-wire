//! Exercise a derived schema containing a published Platform Account on native and Wasm.
use ic_platform_types::{Account, Principal};
use wire::typescript::{SchemaLimits, ValidatedRegistry};
use wire::{Encoding, Limits, decode, encode};

#[derive(wire::typescript::Wire)]
#[wire(crate = "wire")]
struct Payment {
    account: Account,
}

fn main() {
    let graph =
        ValidatedRegistry::from_type::<Payment>("Payment", SchemaLimits::default()).unwrap();
    let declaration = graph.declarations().unwrap();
    for field in ["account", "owner", "subaccount"] {
        assert!(declaration.contains(&format!("readonly \"{field}\"")));
    }
    let limits = Limits {
        bytes: 128,
        allocation: 8192,
        nodes: 64,
        depth: 16,
    };
    for subaccount in [None, Some([0; 32])] {
        let payment = Payment {
            account: Account {
                owner: Principal::anonymous(),
                subaccount,
            },
        };
        let bytes = encode(&payment, Encoding::Payload, limits).unwrap();
        let decoded: Payment = decode(&bytes, Encoding::Payload, limits).unwrap();
        assert_eq!(decoded.account.owner, payment.account.owner);
        assert_eq!(decoded.account.subaccount, subaccount);
        assert_eq!(bytes.len(), if subaccount.is_some() { 37 } else { 4 });
    }
}
