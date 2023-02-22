# How to implement `AsFixedSizeBytes` and `AsDynSizeBytes` traits

> Read more about this in the [API documentation](https://docs.rs/ic-stable-memory/)

`ic-stable-memory` library requires its users to understand the difference between sized and unsized data.

Every stable collection **inlines** the data it stores. For example, `SVec` allocates a big block of stable memory 
and stores each element inside this memory block sequentially. So far, this is exactly how `std` collections work,
but there is a difference. `std` collections use native Rust's data byte representation, which is, as stated by Rust's
documentation, not deterministic and can't be relied on, especially if your program is supposed to persist a lot
of data.

This is why a serialization engine is used in `ic-stable-memory` - to make data persist itself deterministically, even
if a new canister version is compiled with a different Rust compiler version.

`ic-stable-memory` splits all data in two categories:
1. Fixed-size data, that can efficiently and deterministically serialize itself into a buffer of fixed size, known at compile-time.
2. Dynamically-sized data, that can deterministically serialize itself at least somehow.

Types of the first category have to implement `AsFixedSizeBytes` trait. Types of the second category - `AsDynSizeBytes` trait.
Types of the first category may also implement `AsDynSizeBytes`, if needed.

### `AsFixedSizeBytes`
This trait defines a data type that is aware of its byte-size in encoded form and can encode itself exactly so. The
data type itself doesn't have to be `Sized`. Its implementation has to be as performant as possible.

In order to implement it for your data type you can use a derive macro, that will do everything for you - 
`ic_stable_memory::derive::AsFixedSizeBytes`. This macro works just fine for most use-cases, but it does not 
support generic types at the moment. Also, this macro only works if your data type consists purely of fields which
implement `AsFixedSizeBytes` aswell. 

If your data type is generic or you want to implement this trait for a type that contains types which are not
`AsFixedSizeBytes` (for example, types from some other library), this is how you do it:

#### 1. Define the size
Let's see how this trait is defined:
```rust
trait AsFixedSizeBytes {
    const SIZE: usize;
    type Buf: Buffer;

    fn as_fixed_size_bytes(&self, buf: &mut [u8]);
    fn from_fixed_size_bytes(buf: &[u8]) -> Self;
}
```

As you can see, before function implementation, you have to define two things:
1. What size will this data type have when serialized.
2. What buffer type to use, encoding this this data type.

Let's start with the size. Imagine you want to implement this trait for a tuple type `(Principal, u64, u32)`.
> This is an example. In reality this trait already has an implementation for any tuple up to 6 elements

Let's also imagine, that `AsFixedSizeBytes` is not implemented for these three types: `Principal`, `u64`, `u32`.
> In reality this is also not true

We know that we can easily serialize this tuple: for `u64` and `u32` we can use native `.to_le_bytes()` serialization
and for `Principal` we can allocate a byte-buffer of 30 bytes, where the first byte will store the length of this principal
and the rest of them will hold the actual bytes (`Principal`'s max size in bytes is `29`, according to the documentation).

It means, that the size of our data type in encoded form would be:
* `30 bytes` (from `Principal`) + 
* `8 bytes` (from `u64`) + 
* `4 bytes` (from `u32`).

Let's add that to our implementation:
```rust
impl AsFixedSizeBytes for (Principal, u64, u32) {
    const SIZE: usize = 30 + 8 + 4;
}
```

#### 2. Choose the buffer type
For byte buffer type it is pretty straightforward. `Buffer` trait is sealed and is implemented only for two types:
* const generic `[u8; N]`
* `Vec<u8>`

You **always** want to use const generic `[u8; N]`, where `N` is our `SIZE` field, because it is **much** faster. But
if you're implementing this trait for a generic type, than the compiler currently won't let you use const generic expression
inside a generic type, so in generics you use `Vec<u8>` as the buffer type.

Let's add that to our implementation:
```rust
impl AsFixedSizeBytes for (Principal, u64, u32) {
    const SIZE: usize = 30 + 8 + 4;
    type Buf = [u8; Self::SIZE];  // <- use Vec<u8> for generics
}
```

#### 3. Implement the encoding
Now we simply have to use encoding algorithms we've mentioned before to implement this trait completely: 
```rust
impl AsFixedSizeBytes for (Principal, u64, u32) {
    const SIZE: usize = 30 + 8 + 4;
    type Buf = [u8; Self::SIZE];  // <- use Vec<u8> for generics
    
    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        // encode principal
        let principal_bytes = self.0.as_slice();
        buf[0] = principal_bytes.len() as u8;
        buf[1..(1 + principal_bytes.len())].copy_from_slice(principal_bytes);
        
        // encode u64
        buf[(1 + principal_bytes.len())..(1 + principal_bytes.len() + 8)].copy_from_slice(&self.1.to_le_bytes());
        
        // encode u32
        buf[(1 + principal_bytes.len() + 8)..(1 + principal_bytes.len() + 8 + 4)].copy_from_slice(&self.2.to_le_bytes());
    }
    
    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        // decode principal
        let principal_len = buf[0] as usize;
        let principal = Principal::from_slice(&buf[1..(1 + principal_len)]);
        
        // decode u64
        let mut u64_buf = [0u8; 8];
        u64_buf.copy_from_slice(&buf[(1 + principal_len)..(1 + principal_len + 8)]);
        let u64_val = u64::from_le_bytes(u64_buf);
        
        // decode u32
        let mut u32_buf = [0u8; 4];
        u32_buf.copy_from_slice(&buf[(1 + principal_len + 8)..(1 + principal_len + 8 + 4)]);
        let u32_val = u32::from_le_bytes(u32_buf);

        (principal, u64_val, u32_val)
    }
}
```

#### 4. Check yourself
In order to understand if your implementation is correct you can use unit tests:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn encoding_works_fine() {
        let val = (Principal::from_text("vmdca-pqaaa-aaaaf-aabzq-cai"), 100u64, 15u32);
        
        let mut buf = [0u8; 30 + 8 + 4];
        val.as_fixed_size_bytes(&mut buf);
        
        let val_copy = <(Principal, u64, u32)>::from_fixed_size_bytes(&buf);
        
        assert_eq(val, val_copy);
    }
}
```

This is a syntetic example. In most scenarios you would have at least some fields which already implement `AsFixedSizeBytes`
and you can use their encoding methods to simplify your job. For generics the implementation is a almost the same, but you
have to make sure that your generic parameter also implements `AsFixedSizeBytes` and use `Vec<u8>` as the buffer type.

### `AsDynSizeBytes`
As was said earlier, all stable data collections require element type to be `AsFixedSizeBytes`, but what do we do,
when we want to store a dynamically-sized data, like `String`? In that case, we implement `AsDynSizeBytes` trait for
this data and add a layer of indirection with `SBox`. `SBox` is the only stable data structure that accepts types
which implement `AsDynSizeBytes` trait.

This trait is, by default, implemented for a bunch of types:
* for all `AsFixedSizeBytes` types;
* for `String`
* for `Vec<u8>`

> You can disable these default implementation, by enabling `custom_dyn_encoding` feature on this crate:
> ```toml
> ic-stable-memory = { version = "0.4", features = ["custom_dyn_encoding"] }
> ```
> In that case you will have to implement this trait manually for all types.

Implementing this trait is a pretty simple task. First of all, you can use one of two derive macros:
1. `ic_stable_memory::derive::CandidAsDynSizeBytes` will implement this trait for any type that already implements 
`CandidType` and `Deserialize`
2. `ic_stable_memory::derive::FixedSizeAsDynSizeBytes` will implement this trait for any type that already implements
`AsFixedSizeBytes`

Or you can use any other serialization library to implement it. Here is an example of how to use `candid` to manually
implement this trait:

```rust
impl AsDynSizeBytes for Principal {
    #[inline]
    fn as_dyn_size_bytes(&self) -> Vec<u8> {
        candid::encode_one(self).unwrap()
    }

    #[inline]
    fn from_dyn_size_bytes(arr: &[u8]) -> Self {
        ic_stable_memory::encoding::dyn_size::candid_decode_one_allow_trailing(arr).unwrap()
    }
}
```

The only important thing is that deserialization should allow leaving trailing bytes after decoding, because the
buffer that will go into `from_dyn_size_bytes` will often be bigger that the one that was produced by `as_dyn_size_bytes`.