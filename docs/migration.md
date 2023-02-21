# How to migrate from `std` collections

> Warning!
>
> It is strongly suggested to NOT migrate canisters, which already have at least 1GB memory occupied by 
> their state. Migration happens in the `#[post_upgrade]` canister method, so any bad thing that may happen will **BREAK**
> your canister permanently.
> 
> Let them work as they work now. Spin up fresh canisters, that were written with `ic-stable-memory` from scratch and
> find a way to include them into the rest of your app. 
> 
> **But do not migrate old heavy canisters**.

Let's imagine we have a canister like this
```rust
thread_local! {
    static STATE: RefCell<Option<Vec<Principal>>> = RefCell::default();
}

#[init]
fn init() {
    STATE.with(|state| {
        *state.borrow_mut() = Some(Vec::new()); 
    });
}

#[post_upgrade]
fn post_upgrade() {
    STATE.with(|state| {
        *state.borrow_mut() = Some(retrieve_state_from_stable_memory());
    })
}

#[pre_upgrade]
fn pre_upgrade() {
    STATE.with(|state| {
        let state = state.borrow_mut().take().unwrap();
        
        store_state_to_stable_memory(state);
    })
}
```

Migration to `ic-stable-memory` takes **two** canister upgrades. During the first one you transfer all your data from
`std` collection into stable ones. During the second upgrade you clean up the migrating code.

### First upgrade
```rust
thread_local! {
    static STATE: RefCell<Option<SVec<Principal>>> = RefCell::default(); // <- notice Vec changed to SVec
}

// <- notice there is no #[init], you can keep it, but there is no point in that in production

#[post_upgrade]
fn post_upgrade() {
    stable_memory_init(); // <- init stable memory allocator as the FIRST line

    // get the old state from stable memory as usual
    let old_state: Vec<Principal> = retrieve_state_from_stable_memory();
    
    // create a stable collection to move the state there
    let new_state = SVec::<Principal>::new_with_capacity(old_state.len())
        .expect("Out of memory"); // <- this is very dangerous, since it can break your canister

    // move the state to stable collection
    for entry in old_state {
        new_state.push(entry).unwrap();
    }

    // use stable collection instead of the old one
    STATE.with(|state| {
        *state.borrow_mut() = Some(new_state);
    });
}

#[pre_upgrade]
fn pre_upgrade() {
    // take the collection from static as usual
    let state = STATE.with(|state| {
        state.borrow_mut().take().unwrap()
    });
    
    // put it in a SBox
    let boxed_state = SBox::new(state).expect("Out of memory");
    
    // persist the pointer to that SBox between upgrades
    store_custom_data(1, boxed_state);
    
    // persist the memory allocator between upgrades
    stable_memory_pre_upgrade().expect("Out of memory");
}
```

For the next canister upgrade you have to only update the `#[post_upgrade]` hook - everything else stays the same:
```rust
#[post_upgrade]
fn post_upgrade() {
    stable_memory_post_upgrade(); // <- instead of stable_memory_init()

    // retrieve the pointer to the SBox back
    let boxed_state = retrieve_custom_data::<SVec<Principal>>(1).unwrap();
    
    // transform that SBox back into the state
    let state = boxed_state.into_inner();

    // put state into static variable
    STATE.with(|s| {
        *s.borrow_mut() = Some(state);
    });
}
```

This is a simple example, but essentially you will have to complete the same steps for any other situation:
* Move data from `std` collections to `ic-stable-memory` collections, during first canister upgrade.
* Replace the `#[post_upgrade]` method with the one that only works with stable structures, during the second canister upgrade.