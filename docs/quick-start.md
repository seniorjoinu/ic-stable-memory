# Quick start

Developing with `ic-stable-memory` is very similar to developing with `std` data structures. All you have to do is to
replace data structures you're currently using with the stable ones, like this:
```rust
// this
struct State {
    balances: HashMap<Principal, u64>,
    history: Vec<(Principal, Principal, u64)>,
}

// will transform into this
// notice `S` letter before each type
struct State {
    balances: SHashMap<Principal, u64>,
    history: SVec<(Principal, Principal, u64)>,
}
```
Visually changes are minimal, but now the whole state of your canister will be stored in stable memory.

The API of stable data structures is almost identical to `std`'s structures, but there are a couple of differences:

#### 1. Boxing dynamically-sized values

You have to wrap values of dynamically-sized types into a `SBox` smart-pointer (similar to Rust's `Box`, but allocates the 
value on stable memory, instead of heap). So, if you store something dynamically-sized, like strings or byte-buffers, 
you have to do it like this:
```rust
struct State {
    usernames: SHashMap<Principal, SBox<String>>,
}
```

You can read more about fixed-size and dynamic-size types in `ic-stable-memory` [in this article](./encoding.md).

#### 2. Handling `OutOfMemory` errors
In `ic-stable-memory` you have an ability to react to `OutOfMemory` errors (raised when there is no more stable memory
in a subnet) at runtime, like this:
```rust
struct State {
    balances: SHashMap<Principal, u64>,
    history: SVec<(Principal, Principal, u64)>,
}

// ...

state.balances.insert(from, 100)  // <- any method that can allocate memory returns a `Result`
    .expect("Out of memory");     // that can be handled accoring to your needs
```
This allows you to easily define scaling logic and support 100% uptime of your canister. You can read more about that
[in this article](./out-of-memory-error-handling.md).

You may choose to ignore these errors, and simply `.unwrap()` them to make the IC revert the transaction automatically, 
if that is what you wish.

#### 3. Init, pre-upgrade and post-upgrade hooks
You store the state exactly the same way you did it with `std` data structures - by using `thread_local!` static variables
like this:
```rust
thread_local! {
  static STATE: RefCell<Option<State>> = RefCell::default();
}

#[init]
fn init() {
    stable_memory_init();

    STATE.with(|s| {
        *s.borrow_mut() = Some(State::default());
    });
}
```

And when you want to upgrade your canister, all you have to do is to preserve a pointer to this state, so you can find it
after an upgrade is done:
```rust
#[pre_upgrade]
fn pre_upgrade() {
  let state: State = STATE.with(|s| s.borrow_mut().take().unwrap());
  let boxed_state = SBox::new(state).expect("Out of memory");

  store_custom_data(0, boxed_state);

  stable_memory_pre_upgrade().expect("Out of memory");
}

#[post_upgrade]
fn post_upgrade() {
  stable_memory_post_upgrade();

  let state = retrieve_custom_data::<State>(0).unwrap().into_inner();
  STATE.with(|s| {
    *s.borrow_mut() = Some(state);
  });
}
```

#### 4. The rest is the same
Everything else works in exactly the same way as with `std` data structures. There is nothing you should manually allocate
or deallocate. Nothing you have to manually serialize or make into certain byte-size. Yes, you have to implement a couple
of traits for your data (more on this [here](./encoding.md)), but you can use derive macros for most of the cases.