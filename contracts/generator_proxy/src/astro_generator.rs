use cosmwasm_std::{Addr, QuerierWrapper, StdResult};
use cw_storage_plus::Map;
use astroport::generator::UserInfoV2;
use spectrum::adapters::generator::Generator;

const USER_INFO: Map<(&Addr, &Addr), UserInfoV2> = Map::new("user_info");

pub trait GeneratorEx {
    fn query_user_info(&self, querier: &QuerierWrapper, lp_token: &Addr, user: &Addr) -> StdResult<Option<UserInfoV2>>;
}

impl GeneratorEx for Generator {
    fn query_user_info(&self, querier: &QuerierWrapper, lp_token: &Addr, user: &Addr) -> StdResult<Option<UserInfoV2>> {
        USER_INFO.query(querier, self.0.clone(), (lp_token, user))
    }
}
