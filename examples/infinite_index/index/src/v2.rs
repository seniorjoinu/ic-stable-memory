use crate::with_state;
use candid::{CandidType, Deserialize, Principal};
use ic_cdk_macros::update;
use ic_stable_memory::derive::{CandidAsDynSizeBytes, StableType};

#[derive(CandidType, Deserialize, CandidAsDynSizeBytes, StableType, Debug, Clone)]
pub struct User001 {
    pub id: Principal,
    pub name: String,
}

// NEW
#[derive(CandidType, Deserialize, CandidAsDynSizeBytes, StableType, Debug, Clone)]
pub struct User002 {
    pub id: Principal,
    pub first_name: String,
    pub last_name: String,
}

// UPDATED
pub type UserLatest = User002;

// NEW
impl User001 {
    pub fn as_latest(&self) -> UserLatest {
        let name_chunks = self.name.split_whitespace().take(2).collect::<Vec<&str>>();

        if let [first_name, last_name] = name_chunks[..] {
            UserLatest {
                id: self.id,
                first_name: first_name.to_string(),
                last_name: last_name.to_string(),
            }
        } else {
            unreachable!()
        }
    }
}

#[derive(CandidType, Deserialize, CandidAsDynSizeBytes, StableType, Debug, Clone)]
pub enum User {
    V001(User001),
    // NEW
    V002(User002),
}

// UPDATED
impl User {
    pub fn new(user: UserLatest) -> Self {
        Self::V002(user)
    }

    pub fn to_latest(&mut self) {
        match self {
            User::V001(u) => {
                *self = User::V002(u.as_latest());
            }
            User::V002(_) => {}
        }
    }

    pub fn as_latest(&self) -> Self {
        match self {
            User::V001(u) => User::V002(u.as_latest()),
            User::V002(_) => self.clone(),
        }
    }

    pub fn latest_inner_mut(&mut self) -> &mut UserLatest {
        match self {
            User::V002(u) => u,
            _ => unreachable!(),
        }
    }
}

// UPDATED
#[update]
fn update_user(id: Principal, first_name: String, last_name: String) {
    with_state(|state| {
        let mut boxed_user = state.get_mut(&id).expect("Not found");

        boxed_user
            .with(|it| {
                it.to_latest();

                let inner = it.latest_inner_mut();
                inner.first_name = first_name;
                inner.last_name = last_name;
            })
            .expect("Out of memory");
    })
}
