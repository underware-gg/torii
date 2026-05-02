use std::{collections::HashSet, sync::Mutex};

use async_trait::async_trait;
use dojo_types::schema::Ty;
use starknet::core::types::Felt;
use torii_proto::{
    schema::Entity, Achievement, AchievementQuery, Activity, ActivityQuery, AggregationEntry,
    AggregationQuery, Contract, ContractQuery, Controller, ControllerQuery, Event, EventQuery,
    Model, Page, PlayerAchievementEntry, PlayerAchievementQuery, Query, SearchQuery,
    SearchResponse, Token, TokenBalance, TokenBalanceQuery, TokenContract, TokenContractQuery,
    TokenId, TokenQuery, TokenTransfer, TokenTransferQuery, Transaction, TransactionQuery,
};

use crate::{ReadOnlyStorage, StorageError};

#[derive(Debug, Default)]
pub struct ReadOnlyStorageStub {
    token_ids: Mutex<HashSet<TokenId>>,
}

impl ReadOnlyStorageStub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_token_ids(&self, ids: Vec<TokenId>) {
        *self.token_ids.lock().unwrap() = ids.into_iter().collect();
    }
}

#[async_trait]
impl ReadOnlyStorage for ReadOnlyStorageStub {
    fn as_read_only(&self) -> &dyn ReadOnlyStorage {
        self
    }

    async fn model(&self, _world_address: Felt, _selector: Felt) -> Result<Model, StorageError> {
        unimplemented!()
    }

    async fn model_optional(
        &self,
        _world_address: Felt,
        _selector: Felt,
    ) -> Result<Option<Model>, StorageError> {
        unimplemented!()
    }

    async fn models(
        &self,
        _world_addresses: &[Felt],
        _selectors: &[Felt],
    ) -> Result<Vec<Model>, StorageError> {
        Ok(Vec::new())
    }

    async fn token_ids(&self) -> Result<HashSet<TokenId>, StorageError> {
        Ok(self.token_ids.lock().unwrap().clone())
    }

    async fn controllers(
        &self,
        _query: &ControllerQuery,
    ) -> Result<Page<Controller>, StorageError> {
        unimplemented!()
    }

    async fn contracts(&self, _query: &ContractQuery) -> Result<Vec<Contract>, StorageError> {
        unimplemented!()
    }

    async fn tokens(&self, _query: &TokenQuery) -> Result<Page<Token>, StorageError> {
        unimplemented!()
    }

    async fn token_balances(
        &self,
        _query: &TokenBalanceQuery,
    ) -> Result<Page<TokenBalance>, StorageError> {
        unimplemented!()
    }

    async fn token_contracts(
        &self,
        _query: &TokenContractQuery,
    ) -> Result<Page<TokenContract>, StorageError> {
        unimplemented!()
    }

    async fn token_transfers(
        &self,
        _query: &TokenTransferQuery,
    ) -> Result<Page<TokenTransfer>, StorageError> {
        unimplemented!()
    }

    async fn transactions(
        &self,
        _query: &TransactionQuery,
    ) -> Result<Page<Transaction>, StorageError> {
        unimplemented!()
    }

    async fn events(&self, _query: EventQuery) -> Result<Page<Event>, StorageError> {
        unimplemented!()
    }

    async fn entities(&self, _query: &Query) -> Result<Page<Entity>, StorageError> {
        unimplemented!()
    }

    async fn event_messages(&self, _query: &Query) -> Result<Page<Entity>, StorageError> {
        unimplemented!()
    }

    async fn entity_model(
        &self,
        _world_address: Felt,
        _entity_id: Felt,
        _model_selector: Felt,
    ) -> Result<Option<Ty>, StorageError> {
        unimplemented!()
    }

    async fn aggregations(
        &self,
        _query: &AggregationQuery,
    ) -> Result<Page<AggregationEntry>, StorageError> {
        unimplemented!()
    }

    async fn activities(&self, _query: &ActivityQuery) -> Result<Page<Activity>, StorageError> {
        unimplemented!()
    }

    async fn achievements(
        &self,
        _query: &AchievementQuery,
    ) -> Result<Page<Achievement>, StorageError> {
        unimplemented!()
    }

    async fn player_achievements(
        &self,
        _query: &PlayerAchievementQuery,
    ) -> Result<Page<PlayerAchievementEntry>, StorageError> {
        unimplemented!()
    }

    async fn search(&self, _query: &SearchQuery) -> Result<SearchResponse, StorageError> {
        unimplemented!()
    }
}
