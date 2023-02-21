# What to do if your canister is out of stable memory

As you know, stable memory is a limited resource - you're canister might get lucky by getting assigned to a fresh subnet
with a lot of spare stable memory, but even if this is the case, eventually this memory will end, occupied by your and 
other canisters in that subnet.

In `ic-stable-memory` every API method that may potentially allocate stable memory returns `Result`, where `Err` variant 
means that the operaion failed because of `Out of stable memory` error. This means, that you can programmatically react
to these situations, but the question is: 
> "How exactly? What do I do, when my canister runs out of memory?"

### 1. Reverse previous operations
The first thing you have to do, when handling such an error is to **ensure canister state integrity**.

Let's imagine the following example - you're developing a simple token canister using `ic-stable-memory`. The state of such
a canister is a simple map of accounts and their balances
```rust
let mut account_balances = SBTreeMap::<Principal, Nat>::new();
```
A `transfer` transaction should either update both balances (subtract from sender, add to receiver), or none.

```rust
// the transaction in our example can't panic - it should always return Result
#[update]
fn transfer(to: Principal, amount: Nat) -> Result<(), String> {
    let from = caller();
    let from_balance = if let Some(b) = account_balances.get(&from) {
        b.clone()
    } else {
        Nat::from(0)
    };
    
    if from_balance < amount {
        return Err("Not enough funds".into())
    }
    
    let to_balance = if let Some(b) = account_balances.get(&to) {
        b.clone()
    } else {
        Nat::from(0)
    };
    
    // it is easy to handle the first operation
    account_balances
        .insert(from, from_balance - amount)
        .map_err(|_| "Out of stable memory".into())?;
    
    // but the second one is trickier, because we also have to revert the previous one
    match account_balances.insert(to, to_balance + amount) {
        Ok(_) => Ok(()),
        Err(_) => {
            // reset sender's account balance
            account_balances.insert(from, from_balance).unwrap(); // <- safe to unwrap, since we successfully inserted this entry before
            
            Err("Out of stable memory".into())
        }
    }
}
```
Always include reset logic in your canister's methods to keep the state deterministic and sound.
So, the more complex your transactions are, the more "reset"-blocks your code will have.

### 2. Scale horizontally
Transaction reset by itself doesn't give much. In fact, you can achieve almost the same result, simply by calling `.unwrap()`
on `.insert()` result, to make the IC revert the transaction automatically. 

There are situations, when you may delete some of the old data, to be able to continue process transactions with this canister.
For example, if your canister is a history log of some actions, you might want to remove some of the old history entries,
to be able to continue accepting new ones.

But what you can also do, and what is more applicable in most scenarios, is to scale horizontally, deploying a fresh copy
of the same canister and redirecting all new requests to that new canister:

```rust
async fn scale_horizonally() -> Result<(), String> {
    // ...
}

#[update]
async fn transfer(to: Principal, amount: Nat) -> Result<(), String> {
    let from = caller();
    let from_balance = if let Some(b) = account_balances.get(&from) {
        b.clone()
    } else {
        Nat::from(0)
    };
    
    if from_balance < amount {
        return scale_horizonally().await; // <- new line
    }
    
    let to_balance = if let Some(b) = account_balances.get(&to) {
        b.clone()
    } else {
        Nat::from(0)
    };
    
    match account_balances.insert(from, from_balance - amount) {
        Ok(_) => {},
        Err(_) => {
            return scale_horizonally().await; // <- new line
        }
    }
    
    match account_balances.insert(to, to_balance + amount) {
        Ok(_) => Ok(()),
        Err(_) => {
            account_balances.insert(from, from_balance).unwrap(); 
            scale_horizonally().await; // <- new line
        }
    }
}
```