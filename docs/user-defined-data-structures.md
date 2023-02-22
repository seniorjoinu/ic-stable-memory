# How to build your own stable data structures

> At this moment I'm out of capacity to write a full tutorial on this subject, but here are some hints

First of all, try to use existing stable data structures as a base. For example, you can easily build a binary heap using `SVec`.
This is much easier, than building a data structure from scratch.

If you **need** a new data structure, start from reading all the docs on this crate - there is a lot of details scattered
through the whole documentation ([including the API documentation](https://docs.rs/ic-stable-memory/)). Then try to understand [SVec's source code](../src/collections/vec/mod.rs).
This is the simplest data structure you'll find in this crate and it is good for learning.

Use `allocate`, `deallocate` and `reallocate` functions to manage stable memory. Don't forget to implement `StableType::stable_drop` 
and call it from `Drop::drop` checking for `should_stable_drop`. Don't forget to use stable references `SRef` and `SRefMut`
instead of returning copies of data. Try to keep your data structure API and internals as close to its non-stable analog 
as possible.

Write a lot of tests. Drop all stable structures at the end of each test (by using scoping braces `{}`) and check for
memory leaks by asserting that `get_allocated_size()` is equal to `0`. Use fuzzy tests to find unexpected errors.

Make sure your data structure performs exactly the same with `SBox`-ed values as with plain ones.

Ask for an advice in Github issues.