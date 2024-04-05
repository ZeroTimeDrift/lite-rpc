use std::{collections::HashMap, str::FromStr, sync::Arc};

use crate::structures::identity_stakes::IdentityStakesData;
use log::error;
use solana_rpc_client_api::response::RpcVoteAccountStatus;
use solana_sdk::pubkey::{ParsePubkeyError, Pubkey};
use solana_streamer::nonblocking::quic::ConnectionPeerType;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, Default)]
pub struct StakeSummary {
    pub total_stakes: u64,
    pub min_stakes: u64,
    pub max_stakes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct StakeData {
    pub identity_to_stake: HashMap<Pubkey, u64>,
    pub stakes_desc: Vec<(Pubkey, u64)>,
    pub summary: StakeSummary,
}

#[derive(Debug, Clone)]
pub struct StakesStore {
    own_identity: Pubkey,
    data: Arc<RwLock<StakeData>>,
}

impl StakesStore {
    pub fn new(identity: Pubkey) -> Self {
        Self {
            own_identity: identity,
            data: Arc::new(RwLock::new(StakeData::default())),
        }
    }

    pub async fn get_summary(&self) -> StakeSummary {
        self.data.read().await.summary
    }

    /// Stake information for own_identity
    pub async fn get_identity_stakes(&self) -> IdentityStakesData {
        let read_lock = self.data.read().await;
        let own_stake = read_lock.identity_to_stake.get(&self.own_identity);
        let summary = &read_lock.summary;
        own_stake
            .map(|stake| IdentityStakesData {
                peer_type: ConnectionPeerType::Staked,
                stakes: *stake,
                total_stakes: summary.total_stakes,
                min_stakes: summary.min_stakes,
                max_stakes: summary.max_stakes,
            })
            .unwrap_or_default()
    }

    pub async fn get_node_stake(&self, identity: &Pubkey) -> Option<u64> {
        self.data
            .read()
            .await
            .identity_to_stake
            .get(identity)
            .cloned()
    }

    pub async fn get_stake_per_node(&self) -> HashMap<Pubkey, u64> {
        self.data.read().await.identity_to_stake.clone()
    }

    pub async fn get_all_stakes_desc(&self) -> Vec<(Pubkey, u64)> {
        self.data.read().await.stakes_desc.clone()
    }

    pub async fn update_stakes(&self, vote_accounts: RpcVoteAccountStatus) {
        let Ok(mut stakes_desc) = vote_accounts
            .current
            .iter()
            .chain(vote_accounts.delinquent.iter())
            .map(|va| Ok((Pubkey::from_str(&va.node_pubkey)?, va.activated_stake)))
            .collect::<Result<Vec<(Pubkey, u64)>, ParsePubkeyError>>()
        else {
            error!("rpc vote account result contained bad pubkey");
            return;
        };

        stakes_desc.sort_by_key(|(_pk, stake)| std::cmp::Reverse(*stake));

        let id_to_stake: HashMap<Pubkey, u64> = stakes_desc.iter().copied().collect();

        let summary = StakeSummary {
            total_stakes: id_to_stake.values().sum(),
            min_stakes: id_to_stake.values().min().copied().unwrap_or(0),
            max_stakes: id_to_stake.values().max().copied().unwrap_or(0),
        };

        let mut write_lock = self.data.write().await;
        write_lock.summary = summary;
        write_lock.identity_to_stake = id_to_stake;
        write_lock.stakes_desc = stakes_desc;
    }
}
