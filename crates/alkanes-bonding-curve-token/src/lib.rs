use alkanes_runtime::runtime::AlkaneResponder;
use alkanes_runtime::{declare_alkane, imports, message::MessageDispatch, storage::StoragePointer};
#[allow(unused_imports)]
use alkanes_runtime::{
    println,
    stdio::{stdout, Write},
};
use alkanes_std_factory_support::MintableToken;
use alkanes_support::{id::AlkaneId, response::CallResponse, storage::StorageMap};
use anyhow::{anyhow, Result};
use bitcoin::consensus::Decodable; // For Transaction::consensus_decode
use bitcoin::Transaction;
use hex;
use metashrew_support::compat::to_arraybuffer_layout;
use metashrew_support::index_pointer::KeyValuePointer;
use std::sync::Arc;

// Storage key helpers
fn owner_pointer() -> StoragePointer {
    StoragePointer::wrap(&"/owner".as_bytes().to_vec())
}

const BALANCES_PREFIX: &[u8] = b"/balances/";
const ALLOWANCES_PREFIX: &[u8] = b"/allowances/";

#[derive(Default)]
pub struct BondingCurveToken(());

impl MintableToken for BondingCurveToken {}

impl AlkaneResponder for BondingCurveToken {}

#[derive(MessageDispatch)]
enum BondingCurveTokenMessage {
    #[opcode(0)]
    Initialize {
        name: String,
        symbol: String,
        total_supply: u128,
    },

    #[opcode(99)]
    #[returns(String)]
    GetName,

    #[opcode(100)]
    #[returns(String)]
    GetSymbol,

    #[opcode(101)]
    #[returns(u128)]
    GetTotalSupply,

    #[opcode(102)] // BalanceOf
    #[returns(u128)]
    BalanceOf { address: AlkaneId },

    #[opcode(103)] // Transfer
    #[returns(bool)]
    Transfer { to: AlkaneId, amount: u128 },

    #[opcode(104)] // Approve
    #[returns(bool)]
    Approve { spender: AlkaneId, amount: u128 },

    #[opcode(105)] // Allowance
    #[returns(u128)]
    Allowance {
        owner_address: AlkaneId,
        spender_address: AlkaneId,
    },

    #[opcode(106)] // TransferFrom
    #[returns(bool)]
    TransferFrom {
        from_address: AlkaneId,
        to_address: AlkaneId,
        amount: u128,
    },

    #[opcode(107)] // BuyTokens
    #[returns(bool)]
    BuyTokens {
        deployer_btc_address_script_hex: String,
        min_token_amount_to_buy: u128,
    },

    #[opcode(108)] // SellTokens
    #[returns(bool)]
    SellTokens { amount: u128 },
}

impl BondingCurveToken {
    // Helper for host calls (Transaction Data)
    fn _get_current_transaction_bytes() -> Result<Vec<u8>> {
        let tx_len = unsafe { imports::__request_transaction() };
        if tx_len <= 0 {
            return Err(anyhow!(
                "Failed to request transaction or transaction is empty"
            ));
        }
        let mut tx_data = vec![0u8; tx_len as usize];
        unsafe { imports::__load_transaction(tx_data.as_mut_ptr() as i32) };
        Ok(tx_data)
    }

    // Owner storage methods
    fn _get_owner(&self) -> Result<AlkaneId> {
        let owner_bytes = owner_pointer().get();
        if owner_bytes.is_empty() {
            Err(anyhow!("Owner not set"))
        } else {
            AlkaneId::try_from(owner_bytes.as_ref().clone())
        }
    }

    fn _set_owner(&self, owner_id: &AlkaneId) {
        let mut pointer = owner_pointer();
        pointer.set(Arc::new((*owner_id).into()));
    }

    // Balances storage methods
    fn _balances_map(&self) -> StorageMap {
        StorageMap::default()
    }

    fn _get_balance(&self, address: &AlkaneId) -> u128 {
        let address_bytes: Vec<u8> = (*address).into();
        if let Some(balance_bytes) = self._balances_map().get(&address_bytes) {
            u128::from_le_bytes(balance_bytes.as_slice().try_into().unwrap_or([0u8; 16]))
        } else {
            0
        }
    }

    fn _set_balance(&self, address: &AlkaneId, amount: u128) {
        let mut balances = self._balances_map();
        let address_bytes: Vec<u8> = (*address).into();
        let amount_bytes = amount.to_le_bytes().to_vec();
        balances.set(&address_bytes, &amount_bytes);
    }

    // Allowances storage methods
    fn _allowances_map_prefix_for_owner(owner_address: &AlkaneId) -> Vec<u8> {
        let owner_bytes: Vec<u8> = (*owner_address).into();
        [ALLOWANCES_PREFIX, owner_bytes.as_slice()].concat()
    }

    fn _allowances_map_for_owner(&self, _owner_address: &AlkaneId) -> StorageMap {
        StorageMap::default()
    }

    fn _get_allowance(&self, owner_address: &AlkaneId, spender_address: &AlkaneId) -> u128 {
        let allowances = self._allowances_map_for_owner(owner_address);
        let spender_bytes: Vec<u8> = (*spender_address).into();
        if let Some(allowance_bytes) = allowances.get(&spender_bytes) {
            u128::from_le_bytes(allowance_bytes.as_slice().try_into().unwrap_or([0u8; 16]))
        } else {
            0
        }
    }

    fn _set_allowance(&self, owner_address: &AlkaneId, spender_address: &AlkaneId, amount: u128) {
        let mut owner_allowances = self._allowances_map_for_owner(owner_address);
        let spender_bytes: Vec<u8> = (*spender_address).into();
        let amount_bytes = amount.to_le_bytes().to_vec();
        owner_allowances.set(&spender_bytes, &amount_bytes);
    }

    // Existing methods
    fn initialize(&self, name: String, symbol: String, total_supply: u128) -> Result<CallResponse> {
        let context = self.context()?;
        let response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        // Set name and symbol using the MintableToken trait
        <Self as MintableToken>::set_name_and_symbol_str(self, name, symbol);

        // Set the caller as the owner
        let owner_id = context.caller.clone();
        self._set_owner(&owner_id);

        // Update total supply using the MintableToken trait
        // This ensures the value read by `MintableToken::total_supply()` is correct.
        <Self as MintableToken>::increase_total_supply(self, total_supply)?;

        // Set the owner's balance to the total_supply
        self._set_balance(&owner_id, total_supply);

        // The response.alkanes is not modified here for initial minting.
        // Tokens are tracked internally in the balances_map.
        Ok(response)
    }

    fn get_name(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        response.data = <Self as MintableToken>::name(self).into_bytes().to_vec();

        Ok(response)
    }

    fn get_symbol(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        response.data = <Self as MintableToken>::symbol(self).into_bytes().to_vec();

        Ok(response)
    }

    fn get_total_supply(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        response.data = <Self as MintableToken>::total_supply(self)
            .to_le_bytes()
            .to_vec();

        Ok(response)
    }

    // ERC20-like functions
    fn balance_of(&self, address: AlkaneId) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());
        let balance = self._get_balance(&address);
        response.data = balance.to_le_bytes().to_vec();
        Ok(response)
    }

    fn transfer(&self, to: AlkaneId, amount: u128) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());
        let sender = context.caller.clone();

        if sender == to {
            // Per instruction: "Cannot transfer to self".
            // Returning Err is one way, another is false response.data as with insufficient funds.
            // Let's stick to the pattern of returning false for business logic failures.
            // For now, matching the prompt's original suggestion of Err.
            return Err(anyhow!("Cannot transfer to self"));
        }

        let sender_balance = self._get_balance(&sender);
        if sender_balance < amount {
            response.data = vec![0x00]; // false
            return Ok(response);
        }

        self._set_balance(&sender, sender_balance - amount);
        let receiver_balance = self._get_balance(&to);
        self._set_balance(&to, receiver_balance + amount);

        response.data = vec![0x01]; // true
        Ok(response)
    }

    fn approve(&self, spender: AlkaneId, amount: u128) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());
        let owner = context.caller.clone();

        self._set_allowance(&owner, &spender, amount);

        response.data = vec![0x01]; // true
        Ok(response)
    }

    fn allowance(
        &self,
        owner_address: AlkaneId,
        spender_address: AlkaneId,
    ) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());
        let allowance_amount = self._get_allowance(&owner_address, &spender_address);
        response.data = allowance_amount.to_le_bytes().to_vec();
        Ok(response)
    }

    fn transfer_from(
        &self,
        from_address: AlkaneId,
        to_address: AlkaneId,
        amount: u128,
    ) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());
        let spender = context.caller.clone();

        if from_address == to_address {
            // Consistent with transfer, returning Err.
            return Err(anyhow!("Cannot transfer to self"));
        }

        let from_balance = self._get_balance(&from_address);
        if from_balance < amount {
            response.data = vec![0x00]; // false
            return Ok(response);
        }

        let current_allowance = self._get_allowance(&from_address, &spender);
        if current_allowance < amount {
            response.data = vec![0x00]; // false
            return Ok(response);
        }

        self._set_allowance(&from_address, &spender, current_allowance - amount);
        self._set_balance(&from_address, from_balance - amount);
        let to_balance = self._get_balance(&to_address);
        self._set_balance(&to_address, to_balance + amount);

        response.data = vec![0x01]; // true
        Ok(response)
    }

    fn buy_tokens(
        &self,
        deployer_btc_address_script_hex: String,
        min_token_amount_to_buy: u128,
    ) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());
        let buyer = context.caller.clone();

        // 1. Get current transaction bytes
        let tx_bytes = match Self::_get_current_transaction_bytes() {
            Ok(bytes) => bytes,
            Err(_e) => {
                // Consider logging e if a logging mechanism was available
                response.data = vec![0x00]; // false
                return Ok(response);
            }
        };

        // 2. Deserialize the transaction
        let tx: Transaction = match Transaction::consensus_decode(&mut &tx_bytes[..]) {
            Ok(t) => t,
            Err(_e) => {
                // Consider logging e
                response.data = vec![0x00]; // false
                return Ok(response);
            }
        };

        // 3. Iterate outputs and find matching payment to deployer
        let mut btc_paid_to_deployer: u64 = 0;
        let deployer_script_bytes = match hex::decode(&deployer_btc_address_script_hex) {
            Ok(bytes) => bytes,
            Err(_) => {
                response.data = vec![0x00]; // false, invalid hex
                return Ok(response);
            }
        };

        for output in tx.output {
            if output.script_pubkey.as_bytes() == deployer_script_bytes.as_slice() {
                btc_paid_to_deployer += output.value.to_sat(); // Accumulate if multiple outputs match
            }
        }

        if btc_paid_to_deployer == 0 {
            response.data = vec![0x00]; // false, no payment to specified deployer script
            return Ok(response);
        }

        // 4. Calculate token amount & check minimum
        let tokens_to_purchase = btc_paid_to_deployer as u128; // 1 satoshi = 1 token
        if tokens_to_purchase < min_token_amount_to_buy {
            response.data = vec![0x00]; // false, minimum not met
            return Ok(response);
        }

        // 5. Get owner (deployer) AlkaneId
        let owner_id = match self._get_owner() {
            Ok(id) => id,
            Err(_) => {
                // Owner not set, critical error
                return Err(anyhow!("Contract owner not set, cannot process buy."));
            }
        };

        // 6. Execute transferFrom logic (tokens from owner to buyer)
        // This relies on the owner having approved the contract to spend these tokens.
        // The spender is the contract itself (`context.myself`).
        let contract_as_spender = context.myself.clone();

        let owner_balance = self._get_balance(&owner_id);
        if owner_balance < tokens_to_purchase {
            response.data = vec![0x00]; // false, owner has insufficient balance
            return Ok(response);
        }
        let current_allowance = self._get_allowance(&owner_id, &contract_as_spender);
        if current_allowance < tokens_to_purchase {
            response.data = vec![0x00]; // false, contract not approved by owner for this amount
            return Ok(response);
        }

        self._set_allowance(
            &owner_id,
            &contract_as_spender,
            current_allowance - tokens_to_purchase,
        );
        self._set_balance(&owner_id, owner_balance - tokens_to_purchase);
        let buyer_balance = self._get_balance(&buyer);
        self._set_balance(&buyer, buyer_balance + tokens_to_purchase);

        // (Optional: Emit TransferEvent if logging was re-enabled)

        response.data = vec![0x01]; // true, purchase successful
        Ok(response)
    }

    fn sell_tokens(&self, amount: u128) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());
        let seller = context.caller.clone(); // The one calling sell_tokens

        // 1. Get the contract's owner (the recipient of the sold tokens)
        let owner_id = match self._get_owner() {
            Ok(id) => id,
            Err(_) => {
                // Owner not set, critical error
                return Err(anyhow!("Contract owner not set, cannot process sell."));
            }
        };

        // Ensure seller is not selling to themselves if owner is the seller
        if seller == owner_id {
            response.data = vec![0x00]; // false, selling to self is not a valid operation here
            return Ok(response);
        }

        if amount == 0 {
            // Selling zero tokens is a no-op
            response.data = vec![0x01]; // true, as technically nothing failed
            return Ok(response);
        }

        // 2. The contract itself (`context.myself`) will act as the spender
        let contract_as_spender = context.myself.clone();

        // 3. Check seller's balance
        let seller_balance = self._get_balance(&seller);
        if seller_balance < amount {
            response.data = vec![0x00]; // false, seller has insufficient balance
            return Ok(response);
        }

        // 4. Check allowance: seller must have approved the contract to spend this amount
        let current_allowance = self._get_allowance(&seller, &contract_as_spender);
        if current_allowance < amount {
            response.data = vec![0x00]; // false, contract not approved by seller for this amount
            return Ok(response);
        }

        // 5. Update allowance (decrease what the contract can spend from seller)
        self._set_allowance(&seller, &contract_as_spender, current_allowance - amount);

        // 6. Update balances: debit seller, credit owner
        self._set_balance(&seller, seller_balance - amount);
        let owner_balance = self._get_balance(&owner_id);
        self._set_balance(&owner_id, owner_balance + amount);

        response.data = vec![0x01]; // true, sale successful (tokens transferred to owner)
        Ok(response)
    }
}

declare_alkane! {
    impl AlkaneResponder for BondingCurveToken {
        type Message = BondingCurveTokenMessage;
    }
}
