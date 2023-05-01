## Schema evolution (migration) example

This example demostrates how you can use ic-stable-memory to migrate your schema very easily.
This approach suits very well the idea of orthogonal persistence. Also it is efficient, flexible and reliable.

The core idea is that instead of reading the whole stable memory and migrating each record in one big operation 
it is better to leave the data as it is and only migrate those pieces of it which are accessed.

One may ask: 
> but this way our data will be inconsistent - some entries will migrate to version N+1, but others will still be of version N

To which the solution is simple - let's just make our code version-aware. Let's make it aware that sometimes it may 
load entries of old version and this is perfectly fine.

### Instructions
* `dfx deploy schema_evolution`
* open Candid UI in your browser
* create a couple of users
* notice that returned user type is `User::V001`

This example requires a canister code upgrade. For that to happen we have two source code files `src/v1.rs` and `src/v2.rs`
and two candid spec files: `v1.did` and `v2.did`. Initially everything is set so the source code `v1` will be compiled
and installed. In order to upgrade the canister's code, change these lines in `src/actor.rs`:
```rust
mod v1;
use v1::{User, UserLatest}

/* 
mod v2;
use v2::{User, UserLatest}
*/
``` 
So they look like this:
```rust
/* 
mod v1;
use v1::{User, UserLatest}
*/

mod v2;
use v2::{User, UserLatest}
```

* change `dfx.json > canisters > schema_evolution > candid` to `v2.did`.

* hit `dfx deploy schema_evolution`
* reload Candid UI page
* notice that the interface is different now (name -> first name + last name)
* fetch previously created users
* notice that returned user type is `User::V002` now
* try updating these users, notice that everything works just fine
* !!! profit