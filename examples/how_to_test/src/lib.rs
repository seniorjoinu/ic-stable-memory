use std::ops::Deref;

use candid::{Nat, Principal};
use ic_stable_memory::{collections::SBTreeMap, primitive::s_ref::SRef};

struct TokenState {
    balances: SBTreeMap<Principal, Nat>,
}

impl TokenState {
    pub fn new() -> Self {
        Self {
            balances: SBTreeMap::default(),
        }
    }

    pub fn mint(&mut self, to: Principal, qty: Nat) {
        if let Some(mut balance) = self.balances.get_mut(&to) {
            *balance = qty + balance.clone();
            return;
        }

        self.balances.insert(to, qty).expect("Out of memory");
    }

    pub fn burn(&mut self, from: &Principal, qty: Nat) {
        if let Some(mut balance) = self.balances.get_mut(from) {
            if balance.gt(&qty) {
                *balance = balance.clone() - qty;
            } else {
                panic!("Not enough funds");
            }

            return;
        }

        panic!("Not enough funds");
    }

    pub fn transfer(&mut self, from: &Principal, to: Principal, qty: Nat) {
        self.burn(from, qty.clone());
        self.mint(to, qty);
    }

    pub fn balance_of(&self, of: &Principal) -> Option<SRef<Nat>> {
        self.balances.get(of)
    }
}

#[cfg(test)]
mod tests {
    use candid::{Nat, Principal};
    use ic_stable_memory::{
        _debug_print_allocator, _debug_validate_allocator, init_allocator, stable_memory_init,
    };

    use crate::TokenState;

    fn test_body<F: FnOnce()>(and_then: F) {
        let mut state = TokenState::new();
        let user_1 = Principal::management_canister();
        let user_2 = Principal::anonymous();

        state.mint(user_1, Nat::from(10));
        state.transfer(&user_1, user_2, Nat::from(5));
        state.burn(&user_1, Nat::from(1));

        assert_eq!(state.balance_of(&user_1).unwrap().clone(), Nat::from(4));
        assert_eq!(state.balance_of(&user_2).unwrap().clone(), Nat::from(5));

        and_then();
    }

    #[test]
    fn it_works() {
        stable_memory_init();

        test_body(|| {});

        _debug_validate_allocator();
    }

    #[test]
    fn it_works_limited_memory() {
        init_allocator(1);

        test_body(|| _debug_print_allocator());

        _debug_print_allocator();
        
        _debug_validate_allocator();
    }
}
