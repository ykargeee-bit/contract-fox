use reqwest::blocking::Client;
use serde_json::{json, Value};
use soroban_sdk::xdr::{
    AccountId, HostFunction, InvokeContractArgs, InvokeHostFunctionOp,
    Memo, MuxedAccount, Operation, OperationBody, Preconditions, PublicKey, ReadXdr, ScAddress, ScSymbol,
    ScVal, SequenceNumber, SorobanTransactionData, StringM, Transaction, TransactionEnvelope,
    TransactionExt, TransactionV1Envelope, Uint256, WriteXdr, Int128Parts,
};
use stellar_strkey::ed25519::PublicKey as StrkeyPublicKey;

// We assume these types are available in the crate path where this module is included.
use crate::{NetworkConfig, StellarAidError};

/// Prepares a base64-encoded XDR transaction for donating to a campaign.
pub fn build_donate_transaction(
    donor: &str,
    campaign_id: u64,
    amount: i128,
    network: &NetworkConfig,
) -> Result<String, StellarAidError> {
    // 1. Fetch the donor's current sequence number from Horizon.
    let seq_num = fetch_sequence_number(donor, network.horizon_url)
        .map_err(|e| StellarAidError::NetworkError(format!("Failed to fetch sequence: {}", e)))?;

    // 2. Build the donate operation
    // Using a contract ID from env or a dummy default if not set
    let contract_id_str = std::env::var("DONATION_CONTRACT_ID").unwrap_or_else(|_| "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string());
    let contract_id_bytes = stellar_strkey::Contract::from_string(&contract_id_str)
        .map(|c| c.0)
        .unwrap_or([0u8; 32]);
    let op = build_donate_operation(donor, campaign_id, amount, contract_id_bytes)
        .map_err(|e| StellarAidError::ValidationError(e))?;

    // 3. Build preliminary transaction with minimal fee (for simulation)
    let donor_pubkey = parse_account_id(donor)
        .map_err(|e| StellarAidError::ValidationError(e))?;
        
    let mut tx = Transaction {
        source_account: MuxedAccount::Ed25519(Uint256(donor_pubkey)),
        fee: 100, // Minimal base fee for simulation
        seq_num: SequenceNumber(seq_num + 1),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: vec![op].try_into().unwrap(),
        ext: TransactionExt::V0,
    };

    let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: tx.clone(),
        signatures: vec![].try_into().unwrap(),
    });

    let base64_tx = envelope
        .to_xdr_base64()
        .map_err(|_| StellarAidError::TransactionFailed("Failed to encode tx to base64".to_string()))?;

    // 4. Simulate transaction via Soroban RPC
    let (min_fee, soroban_data, auth_entries) = simulate_transaction(&base64_tx, network.soroban_rpc_url)
        .map_err(|e| StellarAidError::SorobanError { code: -1, message: e })?;

    // 5. Update transaction with simulation results (fees, footprint, and auth)
    tx.fee = min_fee as u32 + 100; // include base fee
    tx.ext = TransactionExt::V1(soroban_data);

    // Apply the required authorization entries to the operation
    let mut op = tx.operations[0].clone();
    if let OperationBody::InvokeHostFunction(mut invoke_op) = op.body {
        invoke_op.auth = auth_entries.try_into().unwrap();
        op.body = OperationBody::InvokeHostFunction(invoke_op);
    }
    tx.operations = vec![op].try_into().unwrap();

    let final_envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx,
        signatures: vec![].try_into().unwrap(),
    });

    final_envelope
        .to_xdr_base64()
        .map_err(|_| StellarAidError::TransactionFailed("Failed to encode final tx to base64".to_string()))
}

fn parse_account_id(address: &str) -> Result<[u8; 32], String> {
    let pk = StrkeyPublicKey::from_string(address)
        .map_err(|e| format!("Invalid address {}: {}", address, e))?;
    Ok(pk.0)
}

fn build_donate_operation(
    donor: &str,
    campaign_id: u64,
    amount: i128,
    contract_id: [u8; 32],
) -> Result<Operation, String> {
    let donor_bytes = parse_account_id(donor)?;
    let donor_address = ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(donor_bytes))));
    let contract_address = ScAddress::Contract(soroban_sdk::xdr::Hash(contract_id));

    let args = vec![
        ScVal::U64(campaign_id),
        ScVal::I128(Int128Parts {
            hi: (amount >> 64) as i64,
            lo: (amount & 0xFFFFFFFFFFFFFFFF) as u64,
        }),
        ScVal::Address(donor_address),
    ];

    let invoke_args = InvokeContractArgs {
        contract_address,
        function_name: ScSymbol("donate".try_into().unwrap()),
        args: args.try_into().unwrap(),
    };

    let host_fn = HostFunction::InvokeContract(invoke_args);
    let op_body = OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
        host_function: host_fn,
        auth: vec![].try_into().unwrap(),
    });

    Ok(Operation {
        source_account: None,
        body: op_body,
    })
}

fn fetch_sequence_number(address: &str, horizon_url: &str) -> Result<i64, String> {
    let url = format!("{}/accounts/{}", horizon_url.trim_end_matches('/'), address);
    let client = Client::new();
    let resp = client.get(&url).send().map_err(|e| e.to_string())?;
    
    if !resp.status().is_success() {
        return Err(format!("Horizon returned status: {}", resp.status()));
    }
    
    let json: Value = resp.json().map_err(|e| e.to_string())?;
    let seq_str = json["sequence"].as_str().ok_or("Missing sequence field")?;
    seq_str.parse::<i64>().map_err(|e| e.to_string())
}

fn simulate_transaction(base64_tx: &str, rpc_url: &str) -> Result<(i64, SorobanTransactionData, Vec<soroban_sdk::xdr::SorobanAuthorizationEntry>), String> {
    let client = Client::new();
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "simulateTransaction",
        "params": {
            "transaction": base64_tx
        }
    });

    let resp = client.post(rpc_url).json(&payload).send().map_err(|e| e.to_string())?;
    let rpc_res: Value = resp.json().map_err(|e| e.to_string())?;

    if let Some(err) = rpc_res.get("error") {
        return Err(err.to_string());
    }

    let result = rpc_res.get("result").ok_or("Missing result in RPC response")?;
    
    // Check if simulation failed
    if let Some(err) = result.get("error") {
        return Err(format!("Simulation failed: {}", err));
    }

    let min_fee = result["minResourceFee"].as_str()
        .unwrap_or("0")
        .parse::<i64>()
        .unwrap_or(0);

    let transaction_data_b64 = result["transactionData"].as_str()
        .ok_or("Missing transactionData in simulation result")?;

    let soroban_data = SorobanTransactionData::from_xdr_base64(transaction_data_b64)
        .map_err(|_| "Failed to parse transactionData XDR")?;

    let mut auth_entries = Vec::new();
    if let Some(results) = result.get("results").and_then(|r| r.as_array()) {
        if let Some(first_result) = results.first() {
            if let Some(auth_array) = first_result.get("auth").and_then(|a| a.as_array()) {
                for auth_val in auth_array {
                    if let Some(auth_b64) = auth_val.as_str() {
                        let entry = soroban_sdk::xdr::SorobanAuthorizationEntry::from_xdr_base64(auth_b64)
                            .map_err(|_| "Failed to parse auth entry XDR")?;
                        auth_entries.push(entry);
                    }
                }
            }
        }
    }

    Ok((min_fee, soroban_data, auth_entries))
}
