use async_trait::async_trait;
use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::call::call;

use crate::{Child, Key, Value};

#[async_trait]
pub trait IIndexCanister {
    async fn set(&self, key: &Key, value: &Value) -> Result<Option<Value>, SetErr>;
    async fn get(&self, key: &Key) -> Option<Value>;
}

#[async_trait]
impl IIndexCanister for Child {
    async fn set(&self, key: &Key, value: &Value) -> Result<Option<Value>, SetErr> {
        call(self.id, "set", (key, value))
            .await
            .map(|(it,)| it)
            .unwrap()
    }

    async fn get(&self, key: &Key) -> Option<Value> {
        call(self.id, "get", (key,)).await.map(|(it,)| it).unwrap()
    }
}

#[async_trait]
impl IIndexCanister for Principal {
    async fn set(&self, key: &Key, value: &Value) -> Result<Option<Value>, SetErr> {
        call(*self, "set", (key, value))
            .await
            .map(|(it,)| it)
            .unwrap()
    }

    async fn get(&self, key: &Key) -> Option<Value> {
        call(*self, "get", (key,)).await.map(|(it,)| it).unwrap()
    }
}

#[derive(CandidType, Deserialize)]
pub enum Side {
    Less,
    More,
}

#[derive(CandidType, Deserialize)]
pub struct SetErr {
    pub side: Side,
    pub new_limit_key: Key,
    pub data_chunk: Vec<(Key, Value)>,
}

#[derive(CandidType, Deserialize)]
pub struct InitReq {
    pub data_chunk: Option<Vec<(Key, Value)>>,
}

impl InitReq {
    pub fn new(data_chunk: Vec<(Key, Value)>) -> Self {
        Self {
            data_chunk: Some(data_chunk),
        }
    }

    pub fn empty() -> Self {
        Self { data_chunk: None }
    }
}
