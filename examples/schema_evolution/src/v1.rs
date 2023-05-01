use crate::with_state;
use candid::{CandidType, Deserialize, Principal};
use ic_cdk_macros::update;
use ic_stable_memory::derive::{CandidAsDynSizeBytes, StableType};

#[derive(CandidType, Deserialize, CandidAsDynSizeBytes, StableType, Debug, Clone)]
pub struct User001 {
    pub id: Principal,
    pub name: String,
}

pub type UserLatest = User001;

#[derive(CandidType, Deserialize, CandidAsDynSizeBytes, StableType, Debug, Clone)]
pub enum User {
    V001(User001),
}

impl User {
    pub fn new(user: UserLatest) -> Self {
        Self::V001(user)
    }

    pub fn to_latest(&mut self) {
        match self {
            User::V001(_) => {}
        }
    }

    pub fn as_latest(&self) -> Self {
        match self {
            User::V001(_) => self.clone(),
            _ => unreachable!(),
        }
    }

    pub fn latest_inner_mut(&mut self) -> &mut UserLatest {
        match self {
            User::V001(u) => u,
            _ => unreachable!(),
        }
    }
}

#[update]
fn update_user(id: Principal, new_name: String) {
    with_state(|state| {
        let mut boxed_user = state.get_mut(&id).expect("Not found");

        boxed_user
            .with(|it| {
                it.to_latest();

                it.latest_inner_mut().name = new_name;
            })
            .expect("Out of memory");
    })
}
