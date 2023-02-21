# How to make your data upgradable

### 1. Identify data types that may change over time
With traditional flow, when you serialize the complete state into stable memory and then deserialize it back, your state 
has a strictly specified and limited lifetime - it lives from one canister upgrade to another. This means that between 
upgrades you have the power to change data types and resolve every problem "on-demand".

With `ic-stable-memory` your data has static lifetime - it will live forever, persisting itself between canister upgrades.
This means that you have to think about possible upgrade vectors beforehand. **Data upgradability is something that every 
developer should think of from day 0.**

### 2. Make your code aware of data versions
Once you've identified data types which may change over time, teach your code to handle different versions of this data.

Consider this example. Let's imagine we have a `User` type, that is pretty simple at the moment, but we know will 
probably hold much more data in future:
```rust
#[derive(StableType, CandidType, Deserialize, CandidAsDynSizeBytes)]
struct User {
    id: u64,
    username: String,
    email: String,
}

let mut users = SHashMap::<u64, SBox<User>>::new();
```
We know, that some day in the future we might want to also store phone numbers of each user. One way to handle it would be
to intoduce a separate state variable for that:
```rust
let mut user_phone_numbers = SHashMap::<u64, PhoneNumber>::new();
```
For some situations this is a reasonable approach. But it means that, the more fields we want to add to this data type,
the more state variables we would have to introduce. This increases code complexity and makes the performance worse for 
some cases (since we have to search multiple collections to gather all the data about a user).

The better approach would be to teach our code, that this data type may appear in different versions of itself:
```rust
#[derive(StableType, CandidType, Deserialize)]
struct UserV001 {
    id: u64,
    username: String,
    email: String,
}

#[derive(StableType, CandidType, Deserialize, CandidAsDynSizeBytes)]
enum User {
    V001(UserV001)
}

let mut users = SHashMap::<u64, SBox<User>>::new();
```

Now, if we want to change this data type, adding a phone number to it, we can simply introduce a new version of it:
```rust
#[derive(StableType, CandidType, Deserialize)]
struct UserV001 {
    id: u64,
    username: String,
    email: String,
}

#[derive(StableType, CandidType, Deserialize)]
struct UserV002 {
    id: u64,
    username: String,
    email: String,
    phone_number: PhoneNumber,
}

#[derive(StableType, CandidType, Deserialize, CandidAsDynSizeBytes)]
enum User {
    V001(UserV001),
    V002(UserV002),
}
```
After that you can react differently to each version:
```rust
match &*users.get(&1).unwrap() {
    User::V001(u) => { /* ... */ },
    User::V002(u) => { /* ... */ }
}
```

Perfect! Now we have both: upgradability and sound code. If we want to add something else later, we would simply introduce
another version of `User` and it should enough.

> Warning!
>
> Upgradability relies heavily on how does your `AsDynSizeBytes` trait implementation works. In this example, `Candid` serialization is used
under the hood. This serialization encodes enums by first sorting their identifiers lexicographically and then writing
an index of the current enum variant in this sorted list to the output buffer. So, if you use `Candid` for dynamic-size 
serialization and carefully follow the lexicographical order coming up with names for new versions, you're fine.
> 
> But if you use a different serialization library for `AsDynSizeBytes`, or do not follow naming conventions, you might want
to implement `AsDynSizeBytes` trait for `User` enum manually, to make sure versions are always correctly (de)serialized.
>
> More on manual implementation of encoding traits for `ic-stable-memory` is [here](./encoding.md).

### 3. Make your data fixed-size
**This part touches performance, more info on which can be found [here](./perfomance.md).**

Now consider a slightly different example. Let's imagine that originaly our `User` struct was defined like this:
```rust
#[derive(StableType, CandidType, Deserialize, CandidAsDynSizeBytes)]
struct User {
    id: u64,
    referal_code: Nat,
    bonus_points: Nat,
    username: String,
    email: String,
}

let mut users = SHashMap::<u64, SBox<User>>::new();
```
We can clearly see, that a lot of fields in this struct are fixed-size (`id`, `referal_code` and `bonus_points`) and it 
would be nice if we could use that in our advantage. What if we separate `User` type in two data types: one for fixed-size
fields and another for dynamic-size data and then nest dynamic part into the fixed one:
```rust
#[derive(StableType, CandidType, Deserialize, CandidAsDynSizeBytes)]
struct UserDetails {
    username: String,
    email: String,
}

#[derive(StableType, AsFixedSizeBytes)]
struct User {
    id: u64,
    referal_code: Nat,
    bonus_points: Nat,
    details: SBox<UserDetails>, // <- store dynamic-sized part as SBox inside the main struct
}

let mut users = SHashMap::<u64, User>::new(); // <- now we can store User directly, without SBox
```
This approach is faster, that the previous one, because of how `SBox`-es work internally.

Now, in order to make this data type upgradable again, we may add versions to `UserDetails` instead of `User`:
```rust
#[derive(StableType, CandidType, Deserialize)]
struct UserDetailsV001 {
    username: String,
    email: String,
}

#[derive(StableType, CandidType, Deserialize, CandidAsDynSizeBytes)]
enum UserDetails {
    V001(UserDetailsV001) // <- our code is version-aware now
}

#[derive(StableType, AsFixedSizeBytes)]
struct User {
    id: u64,
    referal_code: Nat,
    bonus_points: Nat,
    details: SBox<UserDetails>,
}

let mut users = SHashMap::<u64, User>::new();
```

This approach, in fact, is so superiour to other, that you're strongly suggested to include such a version-aware `details`
field in every data type of you're canister's state. Even if you don't think this data can change over time, in most cases
you'll end up with a better performance AND an ability to upgrade this type one day in future.