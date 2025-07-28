use std::sync::Arc;

use alloy::{
    network::Ethereum,
    primitives::{Address, Bytes, U256},
    providers::Provider,
    sol_types::SolValue,
};

use revm::{
    Context, ExecuteEvm, MainBuilder, MainContext,
    context::result::{ExecutionResult, Output},
    database::{AlloyDB, CacheDB, WrapDatabaseAsync},
    primitives::{TxKind, keccak256},
    state::{AccountInfo, Bytecode},
};

use anyhow::{Result, anyhow};

pub fn revm_call<P: Provider + Clone>(
    from: Address,
    to: Address,
    calldata: Bytes,
    cache_db: &mut CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>>,
) -> Result<Bytes> {
    let mut evm = Context::mainnet()
        .with_db(cache_db)
        .modify_tx_chained(|tx| {
            tx.caller = from;
            tx.kind = TxKind::Call(to);
            tx.data = calldata;
            tx.value = U256::ZERO;
        })
        .build_mainnet();

    let ref_tx = evm.replay().unwrap();
    let result = ref_tx.result;

    let value = match result {
        ExecutionResult::Success {
            output: Output::Call(value),
            ..
        } => value,
        result => {
            return Err(anyhow!("execution failed: {result:?}"));
        }
    };

    Ok(value)
}

pub fn init_cache_db<P: Provider + Clone>(
    provider: Arc<P>,
) -> CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>> {
    CacheDB::new(
        WrapDatabaseAsync::new(AlloyDB::new((*provider).clone(), Default::default())).unwrap(),
    )
}

pub async fn init_account_with_bytecode<P: Provider + Clone>(
    address: Address,
    bytecode: Bytecode,
    cache_db: &mut CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>>,
) -> Result<()> {
    let code_hash = bytecode.hash_slow();
    let acc_info = AccountInfo {
        balance: U256::ZERO,
        nonce: 0_u64,
        code: Some(bytecode),
        code_hash,
    };

    cache_db.insert_account_info(address, acc_info);
    Ok(())
}

pub async fn insert_mapping_storage_slot<P: Provider + Clone>(
    contract: Address,
    slot: U256,
    slot_address: Address,
    value: U256,
    cache_db: &mut CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>>,
) -> Result<()> {
    let hashed_balance_slot = keccak256((slot_address, slot).abi_encode());

    cache_db.insert_account_storage(contract, hashed_balance_slot.into(), value)?;
    Ok(())
}

pub async fn hydrate_pool_state<P: Provider + Clone>(
    cache_db: &mut CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>>,
    provider: &Arc<P>,
    pool: Address,
) -> Result<()> {
    // slot0 (position 0)
    let slot0 = provider.get_storage_at(pool, U256::ZERO).await?;
    cache_db.insert_account_storage(pool, U256::from(0), slot0)?;

    // liquidity (slot 2)
    // let liq = provider.get_storage_at(pool, U256::from(2)).await?;
    // cache_db.insert_account_storage(pool, U256::from(2), liq)?;

    Ok(())
}
