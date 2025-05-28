use alkanes_runtime::runtime::AlkaneResponder;
use alkanes_runtime::{declare_alkane, message::MessageDispatch};
#[allow(unused_imports)]
use alkanes_runtime::{
    println,
    stdio::{stdout, Write},
};
use alkanes_std_factory_support::MintableToken;
use alkanes_support::{context::Context, parcel::AlkaneTransfer, response::CallResponse};
use anyhow::Result;
use metashrew_support::compat::{to_arraybuffer_layout, to_passback_ptr};

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
}

impl BondingCurveToken {
    fn initialize(&self, name: String, symbol: String, total_supply: u128) -> Result<CallResponse> {
        // self.observe_initialization()?; // Omitting as per instructions
        let context = self.context()?;
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());

        <Self as MintableToken>::set_name_and_symbol_str(self, name, symbol);

        // Set total supply. MintableToken::increase_total_supply also increases the supply.
        // We are initializing the token, so the total supply should be what's provided.
        <Self as MintableToken>::increase_total_supply(self, total_supply)?;

        // Mint tokens to the deployer (myself)
        let minted_tokens = AlkaneTransfer {
            id: context.myself.clone(),
            value: total_supply,
        };
        response.alkanes.0.push(minted_tokens);

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
}

declare_alkane! {
    impl AlkaneResponder for BondingCurveToken {
        type Message = BondingCurveTokenMessage;
    }
}
