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

More on this in section #4 of this document.

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

#### 4. `StablyType`, `AsFixedSizeBytes` and `AsDynSizeBytes` traits
The last thing that is different is that in order to put something into stable memory, 
this something should implement at least two of these three traits. All your datatypes should
always implement `StableType` trait - this is mandatory. You can use `derive::StableType` derive
macro to make your life easier. Then you have to choose an encoding trait to also implement for your
data: `AsFixedSizeBytes` or `AsDynSizeBytes`. As a beginner, you might want to choose the second one,
because it is easier to implement it. For example, you can use `candid` to make all the hard work for you:
```rust
#[derive(CandidType, Deserialize, StableType, CandidAsDynSizeBytes)]
struct MyStruct {
    // ...
}
```
In this example, `derive::CandidAsDynSizeBytes` derive macro is used to implement `AsDynSizeBytes` trait for
a type that already implements `CandidType`. You can do the same thing in your code. But this solution has
downsides:
* you will have to use `SBox` smart-pointer to store this value (as shown in section #1);
* your performance will suffer.

So, in order to overcome these downsides, you want to implement `AsFixedSizeBytes` for your datatype. When you implement
this trait for your datatype, you don't need to wrap values of this type with `SBox` anymore and you have a huge gain in
performance. This is not always possible, but the main rule of thumb is: **if you can implement `Copy` for your type, 
then you should definitely implement `AsFixedSizeBytes` for it**. More about how to implement these traits can be 
found [here](./encoding.md). 

More about performance tricks is in [this article](./perfomance.md).

#### 5. The rest is the same
Everything else works in exactly the same way as with `std` data structures. There is nothing you should manually allocate
or deallocate memory. Nothing you have to manually serialize or make into certain byte-size.
