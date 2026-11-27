use super::Budget;
use crate::LimitKind;
use crate::{Error, Limits};

#[test]
fn refusal_is_sticky_and_extreme_charges_cannot_wrap() {
    let budget = Budget::new(Limits {
        bytes: 0,
        allocation: usize::MAX,
        nodes: 2,
        depth: 1,
    });
    assert!(!budget.charge_buffer(usize::MAX));
    assert!(!budget.enter(1, 0));
    assert_eq!(budget.error(), Error::Limit(LimitKind::Allocation));
    let budget = Budget::new(Limits {
        bytes: 2,
        allocation: 1,
        nodes: 2,
        depth: 1,
    });
    assert!(!budget.enter(1, 0));
    assert_eq!(budget.error(), Error::Limit(LimitKind::Allocation));
}

#[test]
fn simultaneous_debit_refusals_keep_the_first_reason() {
    let limits = Limits {
        bytes: 0,
        allocation: 16,
        nodes: 2,
        depth: 3,
    };
    for (depth, count, reason) in [
        (4, 3, LimitKind::Depth),
        (3, 3, LimitKind::Nodes),
        (3, 2, LimitKind::Allocation),
    ] {
        let budget = Budget::new(limits);
        assert!(!budget.enter_many(depth, 3, count));
        assert_eq!(budget.error(), Error::Limit(reason));
        assert!(budget.charge_buffer(2));
        assert!(!budget.enter_many(usize::MAX, usize::MAX, usize::MAX));
        assert_eq!(budget.error(), Error::Limit(reason));
    }

    let budget = Budget::new(limits);
    assert!(budget.enter_many(3, 2, 2));
    assert!(!budget.enter(3, 1));
    assert_eq!(budget.error(), Error::Limit(LimitKind::Nodes));
}
