use async_trait::async_trait;
use ic_cdk::api::call::{call, call_with_payment, CallResult};
use ic_cdk::export::candid::{CandidType, Deserialize, Principal};

#[async_trait]
pub trait IManagementCanisterClient {
    async fn create_canister(
        &self,
        req: CreateCanisterRequest,
        cycles: u64,
    ) -> CallResult<(CreateCanisterResponse,)>;
    async fn install_code(&self, req: InstallCodeRequest) -> CallResult<()>;
    async fn update_settings(&self, req: UpdateSettingsRequest) -> CallResult<()>;
}

#[async_trait]
impl IManagementCanisterClient for Principal {
    async fn create_canister(
        &self,
        req: CreateCanisterRequest,
        cycles: u64,
    ) -> CallResult<(CreateCanisterResponse,)> {
        call_with_payment(*self, "create_canister", (req,), cycles).await
    }

    async fn install_code(&self, req: InstallCodeRequest) -> CallResult<()> {
        call(*self, "install_code", (req,)).await
    }

    async fn update_settings(&self, req: UpdateSettingsRequest) -> CallResult<()> {
        call(*self, "update_settings", (req,)).await
    }
}

#[derive(Clone, CandidType, Deserialize)]
pub struct DeployCanisterSettings {
    pub controller: Option<Principal>,
    pub compute_allocation: Option<u64>,
    pub memory_allocation: Option<u64>,
    pub freezing_threshold: Option<u64>,
}

#[derive(Clone, CandidType, Deserialize)]
pub enum CanisterInstallMode {
    install,
    reinstall,
    upgrade,
}

#[derive(Clone, CandidType, Deserialize)]
pub struct CreateCanisterRequest {
    pub settings: Option<DeployCanisterSettings>,
}

#[derive(Clone, CandidType, Deserialize)]
pub struct CreateCanisterResponse {
    pub canister_id: Principal,
}

#[derive(Clone, CandidType, Deserialize)]
pub struct InstallCodeRequest {
    pub canister_id: Principal,
    pub mode: CanisterInstallMode,
    pub wasm_module: Vec<u8>,
    pub arg: Vec<u8>,
}

#[derive(Clone, CandidType, Deserialize)]
pub struct CanisterSettings {
    pub controllers: Vec<Principal>,
}

#[derive(Clone, CandidType, Deserialize)]
pub struct UpdateSettingsRequest {
    pub canister_id: Principal,
    pub settings: CanisterSettings,
}
