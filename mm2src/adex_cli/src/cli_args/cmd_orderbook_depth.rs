use super::*;
use mm2_rpc::data::legacy::OrderbookDepthRequest;

#[derive(Args)]
#[command(about = "Returns the number of asks and bids for the specified trading pairs")]
pub struct OrderbookDepthArgs {
    #[arg(required = true, value_name = "BASE/REL")]
    pairs: Vec<BaseRel>,
}

#[derive(Clone)]
pub struct BaseRel(String, String);

impl From<&mut OrderbookDepthArgs> for OrderbookDepthRequest {
    fn from(value: &mut OrderbookDepthArgs) -> Self {
        OrderbookDepthRequest {
            pairs: value.pairs.drain(..).map(<(String, String)>::from).collect(),
        }
    }
}

impl From<BaseRel> for (String, String) {
    fn from(mut value: BaseRel) -> Self { (take(&mut value.0), take(&mut value.1)) }
}

impl FromStr for BaseRel {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let split = s
            .split_once("/")
            .ok_or_else(|| anyhow!("Failed to get base_rel from: {s}"))?;

        Ok(BaseRel(split.0.to_string(), split.1.to_string()))
    }
}
