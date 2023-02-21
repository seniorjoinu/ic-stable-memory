# How to gain maximum performance out of ic-stable-memory

## 1. Reduce usage of SBox

Most commonly suggested optimization advices list for any programming system always includes these two:
1. Allocate memory as rarely as possible.
2. Use as little indirection as possible.

For `ic-stable-memory` both of these dogmas boil down to a single advice - **use as few `SBox`-es as bossible**.

When you create a `SBox`, the following happens:
1. The value you want to put inside gets serialized using `AsDynSizeBytes` trait, which will probably allocate heap memory.
2. Then `StableMemoryAllocator` allocates stable memory to store the serialized value.

So it is basically two allocations (one on heap and another on stable memory) per create operation.
When you read a `SBox` from stable memory or update it, you trigger one heap allocation again (because of (de)serialization).
All these allocations greately increase your cycles consumption.

### SBox map/set keys

Consider this example. Let's imagine you have a hashmap, where keys are `String`s. `ic-stable-memory` requires
wrapping every dynamically sized data type into a `SBox`, so you would end up with a data structure like this:
```rust
let map = SHashMap::<SBox<String>, u64>::new();
```
If you want to make it cheaper and faster, ask yourself: "are these keys really unbounded in size?".
Because, if they are not and, for example, they can not be longer than `100` ascii characters, you can use some fixed-size
type to store them, for example `[u8; 100]` or [tinystr](https://icu4x.unicode.org/doc/tinystr/index.html).

In that case you would be able to change your hashmap data type to:
```rust
type Key = [u8; 100];
let mut map = SHashMap::<Key, u64>::new();

map.insert(b"key_1", 1).expect("Out of memory");
```

Simpler key types for maps and sets also have an additional benefit of simplifying your code, because of how `Borrow` trait
works. Let's look at the same boxed key example again:
```rust
let map = SHashMap::<SBox<String>, u64>::new();
```
`SBox<T>` implements `Borrow<T>`, so you can search this hashmap simply by using `String` (without wrapping it in `SBox`):
```rust
let value_opt = map.get(&String::from("some key"));
```
But this call still contains a heap allocation. It would be much better if it would be possible to search directly by `&str`. 
`Borrow` trait only allows accessing one layer of indirection down at the time, so searching directly with `&str` won't work:
```rust
let value_opt = map.get(&"some key"); // <- won't compile
```

But when your key data type is not wrapped in `SBox`, `Borrow` can work more efficiently, allowing you to search by slice:
```rust
let map = SHashMap::<[u8; 100], u64>::new();
let value_opt = map.get(&b"some key"); // <- will compile just fine
```

### SBox for other cases
It is often possible to use fixed-size data type as a key for a map, but almost never as a value. Almost always business
data contains something that has dynamic size: some strings, or lists, or maps. General advice here is the same - try using
`SBox`-es as rarely as possible.

Consider this example:
```rust
struct User {
    id: u64,
    username: String,
    tags: Vec<String>,
    last_seen_timestamp: u64,
    is_premium: bool,
}

let users = SBTreeMap::<u64, User>::new(); // <- won't compile
```
In order to store `User` objects without wrapping it in `SBox`, we have to implement `AsFixedSizeBytes` trait for it. But 
it seems impossible, because both `String` and `Vec<u64>` do not implement this trait and therefore cannot be serialized
into a fixed size byte buffer. But this data type also has a lot of fixed size fields (`id`, `last_seen_timestamp` and 
`is_premium`), fast access to which would greately improve the overall performance of our canister. 

It is recommended for most use-cases to divide your data type in two parts: the one that can be serialized as fixed size bytes
and the other that can't be. And then nest one into another using `SBox` *inside* the data type:
```rust
#[derive(CandidType, Deserialize, StableType, CandidAsDynSizeBytes)]
struct UserDetails {
    username: String,
    tags: Vec<String>,
}

#[derive(AsFixedSizeBytes, StableType)]
struct User {
    id: u64,
    last_seen_timestamp: u64,
    is_premium: bool,
    details: SBox<UserDetails>,
}

let users = SBTreeMap::<u64, User>::new(); // <- will compile just fine
```
This approach has a couple of benefits:

#### 1. `SBox` is eager on writes, but lazy on reads, so when you get a `User` object from `users` map like this:
```rust
let user: User = users.get(&10).unwrap();
```
`user`'s `details` field is in the `unitialized` state - nothing was read from the stable memory yet. It will initialize 
itself automatically, when you access the actual data:
```rust
println!("{}", user.details.username);
```
This means, that if your canister, for example, often uses `is_premium` and `last_seen_timestamp` fields, but rarely
uses `details` field, you'll get only good from both worlds: reasonable performance and uncompromised functionality.
#### 2. This approach is very upgrade-friendly. 
You can read more on upgradeability [here](./upgradeability.md).

## 2. Know your application
Another thing to keep in mind, when you want to save some cycles, is to always use the most suitable data collection for the task.
Currently there are `6` non-certified collections: `3` of them are "finite" and the other `3` of them are "infinite".
"Finite" collections (`SVec`, `SHashMap`, `SHashSet`) are faster, but only suitable for situations when the data you
want to store inside them is limited in number. On the other hand, "infinite" collections (`SLog`, `SBTreeMap`, `SBTreeSet`) 
are slower, but can hold as many data entries, as the subnet allows.

So, if you don't know how many users may create a profile in your app, store them in `SBTreeMap`. But if you know, that
this particular canister will store only up to a million (for example) users - store their profiles in `SHashMap`. If you're
building, for example, an NFT marketplace, it would be a good call to store trade history in `SLog`, but to store auction
bids in a `SVec`.

Another thing is usage of standard collections within your stable data. Consider the example from above:
```rust
#[derive(CandidType, Deserialize, StableType, CandidAsDynSizeBytes)]
struct UserDetails {
    username: String,
    tags: Vec<String>,
}
```
It is perfectly fine to use `Vec<String>` inside a struct like that, if you know, that there won't be a lot of tags per 
each user. If this nuber is order of tens - this will work okay. If this number is order of hundreds or more, you better 
move it to `SVec<SBox<String>>` or even introduce a separate collection to show relations between `tags` and `users` in a
more scalable way.